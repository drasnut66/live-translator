//! Backend-neutral streaming contracts and stable-prefix coordination.
//!
//! The whisper.cpp adapter will convert its token output into `TokenPiece`s.
//! This crate deliberately does not depend on a particular ASR runtime or UI.

use std::cmp::min;

#[derive(Debug, Clone, PartialEq)]
pub struct AudioFrame {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    pub timestamp_ms: u64,
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
