use super::microphone::{BufferedPcmCapture, CaptureSampleSink};
use super::AudioFrameSource;
use live_translator_core::{AudioFrame, WHISPER_SAMPLE_RATE};
use std::io::{self, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use tungstenite::{
    accept_hdr, handshake::server::Response, protocol::WebSocket, Message,
};

pub const BROWSER_TAB_WEBSOCKET_ADDR: &str = "127.0.0.1:8765";

/// Receives raw f32 PCM at 16 kHz mono from the browser-tab capture extension
/// over a localhost WebSocket, buffers it through the shared
/// `BufferedPcmCapture`, and emits the same 20 ms `AudioFrame` contract as the
/// microphone and system-audio adapters.
pub struct BrowserTabPcmSource {
    capture: BufferedPcmCapture,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl BrowserTabPcmSource {
    pub fn open() -> io::Result<Self> {
        let capture = BufferedPcmCapture::new(WHISPER_SAMPLE_RATE)?;
        let sink = capture.sample_sink();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let listener = TcpListener::bind(BROWSER_TAB_WEBSOCKET_ADDR).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "failed to listen on {BROWSER_TAB_WEBSOCKET_ADDR} for browser-tab audio: {error}"
                ),
            )
        })?;
        // Non-blocking accept lets the serve loop observe the stop flag even
        // when no client is connected.
        listener.set_nonblocking(true).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("failed to set browser-tab listener non-blocking: {error}"),
            )
        })?;
        let join = thread::Builder::new()
            .name("live-translator-browser-tab-websocket".to_owned())
            .spawn(move || serve_websocket(listener, thread_stop, sink))
            .map_err(|error| {
                io::Error::other(format!("failed to start browser-tab websocket thread: {error}"))
            })?;

        Ok(Self {
            capture,
            stop,
            join: Some(join),
        })
    }

    pub fn dropped_samples(&self) -> u64 {
        self.capture.dropped_samples()
    }

    fn close(&mut self) -> io::Result<()> {
        self.stop.store(true, Ordering::Release);
        self.capture.stop();
        if let Some(join) = self.join.take() {
            join.join()
                .map_err(|_| io::Error::other("browser-tab websocket thread panicked"))?;
        }
        Ok(())
    }
}

impl AudioFrameSource for BrowserTabPcmSource {
    fn next_frame(&mut self, stop: &AtomicBool) -> io::Result<Option<AudioFrame>> {
        self.capture.next_frame(stop)
    }

    fn stop(&mut self) -> io::Result<()> {
        self.close()
    }
}

impl Drop for BrowserTabPcmSource {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

fn serve_websocket(
    listener: TcpListener,
    stop: Arc<AtomicBool>,
    sink: CaptureSampleSink,
) {
    loop {
        if stop.load(Ordering::Acquire) {
            return;
        }
        // The listener is non-blocking for the stop check; restore blocking
        // mode on the accepted stream so the WebSocket handshake and reads
        // behave normally.
        let (stream, _) = match listener.accept() {
            Ok(accepted) => {
                let _ = accepted.0.set_nonblocking(false);
                accepted
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(50));
                continue;
            }
            Err(_) => continue,
        };
        // The extension never sends an Origin on localhost; accept any origin.
        let result = accept_hdr(
            stream,
            |_request: &tungstenite::handshake::server::Request, mut response: Response| {
                response
                    .headers_mut()
                    .insert("Access-Control-Allow-Origin", "*".parse().unwrap());
                Ok(response)
            },
        );
        let mut websocket = match result {
            Ok(websocket) => websocket,
            Err(error) => {
                sink.fail(format!("browser-tab websocket handshake failed: {error}"));
                continue;
            }
        };
        let result = pump_messages(&mut websocket, &stop, &sink);
        match result {
            Ok(()) => {}
            Err(error) => sink.fail(format!("browser-tab websocket stream failed: {error}")),
        }
        if stop.load(Ordering::Acquire) {
            return;
        }
    }
}

fn pump_messages<S: Read + Write>(
    websocket: &mut WebSocket<S>,
    stop: &AtomicBool,
    sink: &CaptureSampleSink,
) -> Result<(), String> {
    loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        match websocket.read() {
            Ok(Message::Binary(bytes)) => {
                sink.push_samples(decode_f32_pcm(&bytes));
            }
            // A client disconnect or reconnect should not terminate the source;
            // return to the accept loop so the session keeps waiting for the
            // next connection.
            Ok(Message::Close(_)) => return Ok(()),
            Ok(Message::Ping(_) | Message::Pong(_) | Message::Text(_) | Message::Frame(_)) => {}
            Err(_) => return Ok(()),
        }
    }
}

fn decode_f32_pcm(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| {
            let value = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            if value.is_finite() {
                value
            } else {
                0.0
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::decode_f32_pcm;

    #[test]
    fn decodes_le_f32_pcm_and_sanitizes_non_finite_values() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&1.0_f32.to_le_bytes());
        bytes.extend_from_slice(&(-0.5_f32).to_le_bytes());
        bytes.extend_from_slice(&f32::NAN.to_le_bytes());

        let decoded = decode_f32_pcm(&bytes);
        assert_eq!(decoded, vec![1.0, -0.5, 0.0]);
    }
}
