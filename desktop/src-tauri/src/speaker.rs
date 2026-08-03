use serde::Serialize;
use speakrs::{ExecutionMode, OwnedDiarizationPipeline};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};

/// One committed ASR segment whose audio we want to label with a speaker.
pub struct PendingSegment {
    pub segment_id: u64,
    pub start_sec: f64,
    pub end_sec: f64,
    /// 16 kHz mono audio for this segment.
    pub samples: Vec<f32>,
}

/// A speaker assignment resolved for a committed segment.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerAssignment {
    pub segment_id: u64,
    pub speaker_id: u32,
}

/// How much buffered audio (seconds) to run through diarization at once.
/// Larger batches improve clustering stability but delay labels.
const BATCH_SECONDS: f64 = 20.0;
/// Keep at most this much audio buffered so a long stream does not grow memory.
const MAX_BUFFER_SECONDS: f64 = 60.0;

enum Job {
    Segment(PendingSegment),
    Shutdown,
}

/// Runs speaker diarization off the ASR thread. The session pushes committed
/// segments; the worker batches them, runs the speakrs pipeline over a rolling
/// audio buffer, and reports a speaker id for each segment.
pub struct SpeakerWorker {
    tx: mpsc::Sender<Job>,
    rx: mpsc::Receiver<SpeakerAssignment>,
    join: Option<JoinHandle<()>>,
}

impl SpeakerWorker {
    pub fn start(
        models_dir: PathBuf,
        stop: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let join = thread::Builder::new()
            .name("live-translator-speaker".to_owned())
            .spawn(move || run_worker(models_dir, rx, result_tx, stop))
            .map_err(|error| format!("failed to start speaker worker: {error}"))?;
        Ok(Self {
            tx,
            rx: result_rx,
            join: Some(join),
        })
    }

    pub fn submit(&self, segment: PendingSegment) {
        let _ = self.tx.send(Job::Segment(segment));
    }

    /// Non-blocking drain of resolved assignments.
    pub fn drain_assignments(&self) -> Vec<SpeakerAssignment> {
        let mut assignments = Vec::new();
        while let Ok(assignment) = self.rx.try_recv() {
            assignments.push(assignment);
        }
        assignments
    }
}

impl Drop for SpeakerWorker {
    fn drop(&mut self) {
        let _ = self.tx.send(Job::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run_worker(
    models_dir: PathBuf,
    rx: mpsc::Receiver<Job>,
    result_tx: mpsc::Sender<SpeakerAssignment>,
    stop: Arc<AtomicBool>,
) {
    let mut pipeline: Option<OwnedDiarizationPipeline> = None;
    let mut buffer: Vec<f32> = Vec::new();
    let mut buffered_sec = 0.0_f64;
    let mut pending: Vec<PendingSegment> = Vec::new();
    // Local speakrs label (e.g. "SPEAKER_00") -> stable app speaker id.
    let mut label_map: HashMap<String, u32> = HashMap::new();
    let mut next_id: u32 = 0;

    while !stop.load(Ordering::Acquire) {
        let job = match rx.recv() {
            Ok(job) => job,
            Err(_) => break,
        };
        let Job::Segment(segment) = job else {
            break;
        };
        let samples_len = segment.samples.len();
        buffered_sec += samples_len as f64 / 16_000.0;
        buffer.extend(segment.samples.iter().copied());
        pending.push(segment);

        if buffered_sec > MAX_BUFFER_SECONDS {
            let drop_samples = ((buffered_sec - BATCH_SECONDS) * 16_000.0) as usize;
            if drop_samples >= buffer.len() {
                buffer.clear();
                buffered_sec = 0.0;
            } else {
                buffer.drain(..drop_samples);
                buffered_sec -= drop_samples as f64 / 16_000.0;
            }
        }

        if buffered_sec >= BATCH_SECONDS && !pending.is_empty() {
            if pipeline.is_none() {
                match OwnedDiarizationPipeline::from_dir(
                    &models_dir,
                    current_execution_mode(),
                ) {
                    Ok(p) => pipeline = Some(p),
                    Err(error) => {
                        eprintln!("speaker pipeline failed to load: {error}");
                        pending.clear();
                        buffer.clear();
                        buffered_sec = 0.0;
                        continue;
                    }
                }
            }
            let Some(pipeline) = pipeline.as_mut() else {
                continue;
            };
            match pipeline.run(&buffer) {
                Ok(result) => emit_assignments(
                    &result.segments,
                    &pending,
                    &mut label_map,
                    &mut next_id,
                    &result_tx,
                ),
                Err(error) => {
                    eprintln!("speaker diarization failed: {error}");
                }
            }
            pending.clear();
            buffer.clear();
            buffered_sec = 0.0;
        }
    }
}

#[cfg(target_os = "macos")]
fn current_execution_mode() -> ExecutionMode {
    ExecutionMode::CoreMl
}

#[cfg(not(target_os = "macos"))]
fn current_execution_mode() -> ExecutionMode {
    ExecutionMode::Cpu
}

fn emit_assignments(
    diar_segments: &[speakrs::segment::Segment],
    pending: &[PendingSegment],
    label_map: &mut HashMap<String, u32>,
    next_id: &mut u32,
    result_tx: &mpsc::Sender<SpeakerAssignment>,
) {
    for segment in pending {
        let best = diar_segments.iter().fold(
            None,
            |best: Option<(&speakrs::segment::Segment, f64)>, diar| {
                let overlap = overlap_seconds(
                    segment.start_sec,
                    segment.end_sec,
                    diar.start,
                    diar.end,
                );
                if overlap <= 0.0 {
                    return best;
                }
                match best {
                    Some((_, best_overlap)) if overlap <= best_overlap => best,
                    _ => Some((diar, overlap)),
                }
            },
        );
        let Some((diar, _)) = best else {
            eprintln!("speaker: segment {} unlabeled", segment.segment_id);
            continue;
        };
        let speaker_id = *label_map
            .entry(diar.speaker.clone())
            .or_insert_with(|| {
                let id = *next_id;
                *next_id += 1;
                id
            });
        let _ = result_tx.send(SpeakerAssignment {
            segment_id: segment.segment_id,
            speaker_id,
        });
    }
}

fn overlap_seconds(a_start: f64, a_end: f64, b_start: f64, b_end: f64) -> f64 {
    (a_end.min(b_end) - a_start.max(b_start)).max(0.0)
}
