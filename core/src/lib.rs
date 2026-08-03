//! Backend-neutral streaming contracts and stable-prefix coordination.
//!
//! The whisper.cpp adapter will convert its token output into `TokenPiece`s.
//! This crate deliberately does not depend on a particular ASR runtime or UI.

use std::cmp::min;
use std::collections::VecDeque;

pub const WHISPER_SAMPLE_RATE: u32 = 16_000;
pub const DEFAULT_AUDIO_FRAME_SAMPLES: usize = 320;

#[derive(Debug, Clone, PartialEq)]
pub struct AudioFrame {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    pub timestamp_ms: u64,
}

/// Incrementally converts little-endian signed 16-bit mono PCM into the
/// f32 frames consumed by the ASR pipeline.
///
/// The decoder may return arbitrary byte boundaries, so this type retains
/// incomplete samples and incomplete frames between reads. Timestamps refer
/// to the beginning of each frame on the decoded audio timeline.
#[derive(Debug, Clone)]
pub struct Pcm16FrameAssembler {
    sample_rate: u32,
    frame_samples: usize,
    pending: Vec<u8>,
    samples_seen: u64,
}

impl Pcm16FrameAssembler {
    pub fn new(sample_rate: u32, frame_samples: usize) -> Self {
        assert!(sample_rate > 0, "sample rate must be positive");
        assert!(frame_samples > 0, "frame size must be positive");

        Self {
            sample_rate,
            frame_samples,
            pending: Vec::new(),
            samples_seen: 0,
        }
    }

    pub fn whisper_defaults() -> Self {
        Self::new(WHISPER_SAMPLE_RATE, DEFAULT_AUDIO_FRAME_SAMPLES)
    }

    pub fn push(&mut self, bytes: &[u8]) -> Vec<AudioFrame> {
        self.pending.extend_from_slice(bytes);
        self.take_frames(false)
    }

    /// Flush a final short frame when the source closes.
    pub fn finish(&mut self) -> Option<AudioFrame> {
        self.take_frames(true).pop()
    }

    fn take_frames(&mut self, include_partial: bool) -> Vec<AudioFrame> {
        let frame_bytes = self.frame_samples * 2;
        let complete_bytes = self.pending.len() - (self.pending.len() % 2);
        let available_bytes = if include_partial {
            complete_bytes
        } else {
            complete_bytes - (complete_bytes % frame_bytes)
        };

        let mut frames = Vec::new();
        let mut offset = 0;
        while offset + frame_bytes <= available_bytes {
            frames.push(self.decode_frame(frame_bytes, offset));
            offset += frame_bytes;
        }

        if include_partial && offset < available_bytes {
            frames.push(self.decode_frame(available_bytes - offset, offset));
            offset = available_bytes;
        }

        if offset > 0 {
            self.pending.drain(..offset);
        }

        if include_partial {
            // A truncated final byte cannot form an i16 sample and is ignored.
            self.pending.clear();
        }

        frames
    }

    fn decode_frame(&mut self, byte_len: usize, offset: usize) -> AudioFrame {
        let timestamp_ms = self.samples_seen * 1000 / u64::from(self.sample_rate);
        let samples = self.pending[offset..offset + byte_len]
            .chunks_exact(2)
            .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]) as f32 / 32_768.0)
            .collect::<Vec<_>>();
        self.samples_seen += samples.len() as u64;

        AudioFrame {
            samples,
            sample_rate: self.sample_rate,
            channels: 1,
            timestamp_ms,
        }
    }
}

/// A small bounded queue for decoded audio. Dropping the oldest frame keeps
/// the recognizer near live input when inference temporarily falls behind.
#[derive(Debug)]
pub struct BoundedAudioQueue {
    capacity: usize,
    frames: VecDeque<AudioFrame>,
    dropped_frames: u64,
}

impl BoundedAudioQueue {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "queue capacity must be positive");
        Self {
            capacity,
            frames: VecDeque::with_capacity(capacity),
            dropped_frames: 0,
        }
    }

    pub fn push(&mut self, frame: AudioFrame) -> Option<AudioFrame> {
        let dropped = if self.frames.len() == self.capacity {
            self.frames.pop_front()
        } else {
            None
        };
        if dropped.is_some() {
            self.dropped_frames += 1;
        }
        self.frames.push_back(frame);
        dropped
    }

    pub fn pop(&mut self) -> Option<AudioFrame> {
        self.frames.pop_front()
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn dropped_frames(&self) -> u64 {
        self.dropped_frames
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenPiece {
    pub id: i32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrPartial {
    pub segment_id: u64,
    pub revision: u64,
    pub stable_text_ja: String,
    pub unstable_text_ja: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrCommit {
    pub segment_id: u64,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text_ja: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StableUpdate {
    pub partial: AsrPartial,
    pub commit: Option<AsrCommit>,
}

#[derive(Debug, Clone)]
pub struct StablePrefixCoordinator {
    segment_id: u64,
    revision: u64,
    start_ms: u64,
    committed: Vec<TokenPiece>,
    previous: Vec<TokenPiece>,
}

impl StablePrefixCoordinator {
    pub fn new(segment_id: u64, start_ms: u64) -> Self {
        Self {
            segment_id,
            revision: 0,
            start_ms,
            committed: Vec::new(),
            previous: Vec::new(),
        }
    }

    /// Compare consecutive hypotheses and expose only the common stable prefix.
    ///
    /// A token becomes committed after it appears in two consecutive hypotheses.
    /// Already committed tokens are monotonic and are never retracted.
    pub fn observe(&mut self, current: &[TokenPiece], end_ms: u64) -> StableUpdate {
        self.revision += 1;
        let agreed_len = common_prefix_len(&self.previous, current);
        let commit_len = min(agreed_len, current.len());

        let commit = if commit_len > self.committed.len() {
            let newly_committed = &current[self.committed.len()..commit_len];
            self.committed.extend_from_slice(newly_committed);
            Some(AsrCommit {
                segment_id: self.segment_id,
                start_ms: self.start_ms,
                end_ms,
                text_ja: pieces_to_text(&self.committed),
            })
        } else {
            None
        };

        self.previous = current.to_vec();
        let keeps_committed_prefix = starts_with_tokens(current, &self.committed);
        let stable_text_ja = pieces_to_text(&self.committed);
        let unstable_text_ja = if keeps_committed_prefix {
            pieces_to_text(&current[self.committed.len()..])
        } else {
            String::new()
        };

        StableUpdate {
            partial: AsrPartial {
                segment_id: self.segment_id,
                revision: self.revision,
                stable_text_ja,
                unstable_text_ja,
            },
            commit,
        }
    }

    /// Commit the remaining hypothesis when VAD detects the end of speech.
    pub fn finish(&mut self, current: &[TokenPiece], end_ms: u64) -> Option<AsrCommit> {
        if current.len() <= self.committed.len() || !starts_with_tokens(current, &self.committed) {
            return None;
        }

        self.committed = current.to_vec();
        Some(AsrCommit {
            segment_id: self.segment_id,
            start_ms: self.start_ms,
            end_ms,
            text_ja: pieces_to_text(&self.committed),
        })
    }
}

fn common_prefix_len(left: &[TokenPiece], right: &[TokenPiece]) -> usize {
    left.iter()
        .zip(right)
        .take_while(|(left, right)| left.id == right.id)
        .count()
}

fn starts_with_tokens(tokens: &[TokenPiece], prefix: &[TokenPiece]) -> bool {
    tokens.len() >= prefix.len()
        && tokens
            .iter()
            .zip(prefix)
            .all(|(token, prefix_token)| token.id == prefix_token.id)
}

fn pieces_to_text(pieces: &[TokenPiece]) -> String {
    pieces.iter().map(|piece| piece.text.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(id: i32, text: &str) -> TokenPiece {
        TokenPiece {
            id,
            text: text.to_owned(),
        }
    }

    fn pcm_bytes(samples: &[i16]) -> Vec<u8> {
        samples
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect()
    }

    #[test]
    fn assembler_handles_arbitrary_read_boundaries() {
        let mut assembler = Pcm16FrameAssembler::new(1_000, 3);
        let bytes = pcm_bytes(&[i16::MIN, 0, i16::MAX, 16_384]);

        assert!(assembler.push(&bytes[..1]).is_empty());
        assert!(assembler.push(&bytes[1..4]).is_empty());
        let frames = assembler.push(&bytes[4..]);

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].samples, vec![-1.0, 0.0, 0.9999695]);
        assert_eq!(frames[0].timestamp_ms, 0);
        let final_frame = assembler.finish().expect("short final frame");
        assert_eq!(final_frame.samples, vec![0.5]);
        assert_eq!(final_frame.timestamp_ms, 3);
    }

    #[test]
    fn assembler_flushes_only_complete_samples() {
        let mut assembler = Pcm16FrameAssembler::new(1_000, 4);
        let mut bytes = pcm_bytes(&[1, 2]);
        bytes.push(0xff);

        assert!(assembler.push(&bytes).is_empty());
        let frame = assembler.finish().expect("two complete samples");
        assert_eq!(frame.samples.len(), 2);
        assert_eq!(assembler.finish(), None);
    }

    #[test]
    fn bounded_queue_drops_oldest_frame_and_counts_it() {
        let mut queue = BoundedAudioQueue::new(2);
        let frame = |timestamp_ms| AudioFrame {
            samples: vec![timestamp_ms as f32],
            sample_rate: WHISPER_SAMPLE_RATE,
            channels: 1,
            timestamp_ms,
        };

        assert!(queue.push(frame(1)).is_none());
        assert!(queue.push(frame(2)).is_none());
        assert_eq!(queue.push(frame(3)).expect("oldest frame").timestamp_ms, 1);
        assert_eq!(queue.dropped_frames(), 1);
        assert_eq!(queue.pop().expect("second frame").timestamp_ms, 2);
        assert_eq!(queue.pop().expect("newest frame").timestamp_ms, 3);
        assert!(queue.is_empty());
    }

    #[test]
    fn requires_two_matching_hypotheses_before_commit() {
        let mut coordinator = StablePrefixCoordinator::new(7, 100);
        let first = vec![token(1, "今日は"), token(2, "晴れ")];
        let second = vec![token(1, "今日は"), token(2, "晴れ"), token(3, "です")];

        assert_eq!(coordinator.observe(&first, 500).commit, None);
        let update = coordinator.observe(&second, 700);

        assert_eq!(update.partial.stable_text_ja, "今日は晴れ");
        assert_eq!(update.partial.unstable_text_ja, "です");
        assert_eq!(
            update.commit,
            Some(AsrCommit {
                segment_id: 7,
                start_ms: 100,
                end_ms: 700,
                text_ja: "今日は晴れ".to_owned(),
            })
        );
    }

    #[test]
    fn never_emits_the_same_commit_twice() {
        let mut coordinator = StablePrefixCoordinator::new(1, 0);
        let hypothesis = vec![token(1, "a"), token(2, "b")];

        coordinator.observe(&hypothesis, 100);
        assert!(coordinator.observe(&hypothesis, 200).commit.is_some());
        assert!(coordinator.observe(&hypothesis, 300).commit.is_none());
    }

    #[test]
    fn finish_commits_unstable_tail() {
        let mut coordinator = StablePrefixCoordinator::new(2, 50);
        let hypothesis = vec![token(1, "a"), token(2, "b")];

        coordinator.observe(&hypothesis, 100);
        let commit = coordinator.finish(&hypothesis, 400);

        assert_eq!(
            commit,
            Some(AsrCommit {
                segment_id: 2,
                start_ms: 50,
                end_ms: 400,
                text_ja: "ab".to_owned(),
            })
        );
    }

    #[test]
    fn committed_text_does_not_shrink_when_asr_temporarily_regresses() {
        let mut coordinator = StablePrefixCoordinator::new(3, 0);
        let stable = vec![token(1, "a"), token(2, "b")];

        coordinator.observe(&stable, 100);
        coordinator.observe(&stable, 200);
        let update = coordinator.observe(&[token(1, "a")], 300);

        assert_eq!(update.partial.stable_text_ja, "ab");
        assert_eq!(update.partial.unstable_text_ja, "");
        assert_eq!(update.commit, None);
    }

    #[test]
    fn finish_rejects_a_hypothesis_that_rewrites_committed_tokens() {
        let mut coordinator = StablePrefixCoordinator::new(4, 0);
        let stable = vec![token(1, "a"), token(2, "b")];

        coordinator.observe(&stable, 100);
        coordinator.observe(&stable, 200);

        assert_eq!(
            coordinator.finish(&[token(9, "x"), token(10, "y"), token(11, "z")], 300),
            None
        );
    }
}
