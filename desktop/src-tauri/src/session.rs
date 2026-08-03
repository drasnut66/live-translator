use crate::asr::whisper::{WhisperEngine, WhisperEngineConfig};
use crate::audio::ffmpeg::{FfmpegPcmSource, FfmpegSourceConfig};
use crate::translation::{TranslationJob, TranslationWorker};
use crate::{is_file, model_directory, project_path};
use live_translator_core::{StablePrefixCoordinator, TokenPiece, WHISPER_SAMPLE_RATE};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::Instant;
use tauri::{AppHandle, Emitter, State};

pub const SESSION_STATUS_EVENT: &str = "session:status";
pub const SUBTITLE_PARTIAL_EVENT: &str = "subtitle:partial";
pub const SUBTITLE_COMMIT_EVENT: &str = "subtitle:commit";

const MODEL_FILE: &str = "ggml-large-v3-turbo-q5_0.bin";
const MINIMUM_WINDOW_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize * 3 / 2;
const STEP_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize;
const MAXIMUM_WINDOW_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize * 8;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatusPayload {
    pub state: String,
    pub message: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SubtitlePartialPayload {
    pub segment_id: u64,
    pub revision: u64,
    pub stable_ja: String,
    pub unstable_ja: String,
    pub input_ms: u64,
    pub decode_ms: u64,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleCommitPayload {
    pub segment_id: u64,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text_ja: String,
}

struct ActiveSession {
    stop: Arc<AtomicBool>,
    join: JoinHandle<()>,
}

#[derive(Default)]
pub struct SessionController {
    active: Mutex<Option<ActiveSession>>,
}

impl SessionController {
    pub fn start(&self, app: AppHandle, input: PathBuf, realtime: bool) -> Result<(), String> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| "session lock is poisoned".to_owned())?;

        if active
            .as_ref()
            .is_some_and(|session| !session.join.is_finished())
        {
            return Err("a recognition session is already running".to_owned());
        }
        if let Some(session) = active.take() {
            let _ = session.join.join();
        }

        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let join = thread::Builder::new()
            .name("live-translator-asr".to_owned())
            .spawn(move || run_session(app, input, realtime, thread_stop))
            .map_err(|error| format!("failed to start recognition thread: {error}"))?;
        *active = Some(ActiveSession { stop, join });
        Ok(())
    }

    pub fn stop(&self) -> Result<(), String> {
        let session = self
            .active
            .lock()
            .map_err(|_| "session lock is poisoned".to_owned())?
            .take();
        let Some(session) = session else {
            return Ok(());
        };

        session.stop.store(true, Ordering::Release);
        session
            .join
            .join()
            .map_err(|_| "recognition thread panicked".to_owned())
    }
}

impl Drop for SessionController {
    fn drop(&mut self) {
        let Ok(active) = self.active.get_mut() else {
            return;
        };
        let Some(session) = active.take() else {
            return;
        };
        session.stop.store(true, Ordering::Release);
        let _ = session.join.join();
    }
}

#[tauri::command]
pub fn start_media_session(
    app: AppHandle,
    sessions: State<'_, SessionController>,
    input: String,
    realtime: Option<bool>,
) -> Result<(), String> {
    let input = PathBuf::from(input);
    if !input.exists() && !looks_like_network_url(&input) {
        return Err(format!("audio input does not exist: {}", input.display()));
    }
    sessions.start(app, input, realtime.unwrap_or(true))
}

#[tauri::command]
pub fn start_test_session(
    app: AppHandle,
    sessions: State<'_, SessionController>,
) -> Result<(), String> {
    let input = std::env::var_os("LIVE_TRANSLATOR_TEST_MEDIA")
        .map(PathBuf::from)
        .unwrap_or_else(|| project_path("../1.flv"));
    if !input.is_file() {
        return Err(format!("test media is unavailable: {}", input.display()));
    }
    sessions.start(app, input, true)
}

#[tauri::command]
pub fn stop_session(sessions: State<'_, SessionController>) -> Result<(), String> {
    sessions.stop()
}

fn run_session(app: AppHandle, input: PathBuf, realtime: bool, stop: Arc<AtomicBool>) {
    emit_status(&app, "loading", None);

    let model = model_directory(&app).join(MODEL_FILE);
    if !is_file(&model) {
        emit_status(
            &app,
            "error",
            Some(format!("Whisper model is missing: {}", model.display())),
        );
        return;
    }

    let mut engine = match WhisperEngine::load(model, WhisperEngineConfig::default()) {
        Ok(engine) => engine,
        Err(error) => {
            emit_status(&app, "error", Some(error.to_string()));
            return;
        }
    };

    let mut source_config = FfmpegSourceConfig::new(input.to_string_lossy());
    source_config.realtime = realtime;
    let mut source = match FfmpegPcmSource::spawn(source_config) {
        Ok(source) => source,
        Err(error) => {
            emit_status(&app, "error", Some(error.to_string()));
            return;
        }
    };

    emit_status(&app, "running", None);
    let translation = TranslationWorker::start(app.clone(), Arc::clone(&stop));
    let mut segment_id = 1_u64;
    let mut coordinator = StablePrefixCoordinator::new(segment_id, 0);
    let mut audio = Vec::with_capacity(MAXIMUM_WINDOW_SAMPLES);
    let mut latest_hypothesis: Vec<TokenPiece> = Vec::new();
    let mut total_samples = 0_usize;
    let mut next_decode = MINIMUM_WINDOW_SAMPLES;
    let session_started = Instant::now();

    while !stop.load(Ordering::Acquire) {
        let frame = match source.next_frame() {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(error) => {
                emit_status(&app, "error", Some(error.to_string()));
                return;
            }
        };

        if audio.len() + frame.samples.len() > MAXIMUM_WINDOW_SAMPLES {
            let input_ms = total_samples as u64 * 1000 / u64::from(WHISPER_SAMPLE_RATE);
            emit_finished_segment(
                &app,
                &translation,
                &mut coordinator,
                &latest_hypothesis,
                input_ms,
            );
            segment_id += 1;
            coordinator = StablePrefixCoordinator::new(segment_id, input_ms);
            audio.clear();
            latest_hypothesis.clear();
            next_decode = total_samples + MINIMUM_WINDOW_SAMPLES;
        }

        total_samples += frame.samples.len();
        audio.extend(frame.samples);
        if total_samples < next_decode {
            continue;
        }

        let decode_started = Instant::now();
        let hypothesis = match engine.transcribe(&audio) {
            Ok(hypothesis) => hypothesis,
            Err(error) => {
                emit_status(&app, "error", Some(error.to_string()));
                return;
            }
        };
        let input_ms = total_samples as u64 * 1000 / u64::from(WHISPER_SAMPLE_RATE);
        let update = coordinator.observe(&hypothesis, input_ms);
        let decode_ms = decode_started.elapsed().as_millis() as u64;
        let _ = app.emit(
            SUBTITLE_PARTIAL_EVENT,
            SubtitlePartialPayload {
                segment_id: update.partial.segment_id,
                revision: update.partial.revision,
                stable_ja: update.partial.stable_text_ja,
                unstable_ja: update.partial.unstable_text_ja,
                input_ms,
                decode_ms,
            },
        );
        if let Some(commit) = update.commit {
            emit_commit(&app, &translation, commit);
        }
        latest_hypothesis = hypothesis;
        while next_decode <= total_samples {
            next_decode += STEP_SAMPLES;
        }
    }

    if !stop.load(Ordering::Acquire) {
        let input_ms = total_samples as u64 * 1000 / u64::from(WHISPER_SAMPLE_RATE);
        emit_finished_segment(
            &app,
            &translation,
            &mut coordinator,
            &latest_hypothesis,
            input_ms,
        );
    }
    let _ = source.stop();
    drop(translation);
    let elapsed_ms = session_started.elapsed().as_millis() as u64;
    emit_status(
        &app,
        "stopped",
        Some(format!("session elapsed: {elapsed_ms} ms")),
    );
}

fn emit_finished_segment(
    app: &AppHandle,
    translation: &TranslationWorker,
    coordinator: &mut StablePrefixCoordinator,
    latest_hypothesis: &[TokenPiece],
    input_ms: u64,
) {
    if let Some(commit) = coordinator.finish(latest_hypothesis, input_ms) {
        emit_commit(app, translation, commit);
    }
}

fn emit_commit(
    app: &AppHandle,
    translation: &TranslationWorker,
    commit: live_translator_core::AsrCommit,
) {
    let translation_job = TranslationJob {
        segment_id: commit.segment_id,
        start_ms: commit.start_ms,
        end_ms: commit.end_ms,
        text_ja: commit.text_ja.clone(),
    };
    let _ = app.emit(
        SUBTITLE_COMMIT_EVENT,
        SubtitleCommitPayload {
            segment_id: commit.segment_id,
            start_ms: commit.start_ms,
            end_ms: commit.end_ms,
            text_ja: commit.text_ja,
        },
    );
    translation.submit(translation_job);
}

fn emit_status(app: &AppHandle, state: &str, message: Option<String>) {
    let _ = app.emit(
        SESSION_STATUS_EVENT,
        SessionStatusPayload {
            state: state.to_owned(),
            message,
        },
    );
}

fn looks_like_network_url(input: &Path) -> bool {
    input
        .to_string_lossy()
        .get(..8)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("https://"))
        || input
            .to_string_lossy()
            .get(..7)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("http://"))
        || input
            .to_string_lossy()
            .get(..7)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("rtsp://"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_supported_network_sources_case_insensitively() {
        assert!(looks_like_network_url(&PathBuf::from(
            "https://example.test/live.m3u8"
        )));
        assert!(looks_like_network_url(&PathBuf::from(
            "RTSP://example.test/live"
        )));
    }

    #[test]
    fn rejects_non_network_paths_and_unsupported_schemes() {
        assert!(!looks_like_network_url(&PathBuf::from("../1.flv")));
        assert!(!looks_like_network_url(&PathBuf::from(
            "ftp://example.test/live"
        )));
    }
}
