// Offscreen document: receives the tab's MediaStreamId, opens it with
// getUserMedia, resamples to 16 kHz mono f32 with an AudioWorklet, and pushes
// raw LE f32 bytes over a localhost WebSocket to the Live Translator app.

const WS_URL = "ws://127.0.0.1:8765/";
const TARGET_RATE = 16000;
const TARGET_CHANNELS = 1;

let socket = null;
let sourceNode = null;
let context = null;
let audioContext = null;
let stream = null;
let wsOpen = false;

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message && message.type === "start") {
    startCapture(message.streamId)
      .then(() => sendResponse({ ok: true }))
      .catch((error) => sendResponse({ error: String(error) }));
    return true; // async response
  }
  if (message && message.type === "stop") {
    stopCapture()
      .then(() => sendResponse({ ok: true }))
      .catch(() => sendResponse({ ok: false }));
    return true;
  }
});

async function startCapture(streamId) {
  if (context) {
    await stopCapture();
  }

  stream = await navigator.mediaDevices.getUserMedia({
    audio: {
      mandatory: {
        chromeMediaSource: "tab",
        chromeMediaSourceId: streamId,
      },
    },
    video: false,
  });
  if (!stream) throw new Error("getUserMedia returned no stream");
  console.log("got tab media stream");

  audioContext = new AudioContext({ sampleRate: 48000 });
  sourceNode = audioContext.createMediaStreamSource(stream);
  await audioContext.audioWorklet.addModule("audio-processor.js");
  const worklet = new AudioWorkletNode(audioContext, "pcm-processor", {
    numberOfInputs: 1,
    numberOfOutputs: 0,
    outputChannelCount: [1],
    processorOptions: {
      targetRate: TARGET_RATE,
      sampleRate: audioContext.sampleRate,
      channels: TARGET_CHANNELS,
    },
  });
  worklet.port.onmessage = (event) => {
    if (!wsOpen || !socket || socket.readyState !== WebSocket.OPEN) return;
    socket.send(event.data);
  };
  sourceNode.connect(worklet);
  // Route the original audio back to the speakers so the user still hears the
  // tab. tabCapture redirects the tab's audio into this stream, so without
  // this the tab becomes silent.
  sourceNode.connect(audioContext.destination);

  context = { worklet };
  await connectSocket();
  console.log("capture running, websocket open");
}

function connectSocket() {
  return new Promise((resolve, reject) => {
    socket = new WebSocket(WS_URL);
    socket.binaryType = "arraybuffer";
    socket.onopen = () => {
      wsOpen = true;
      resolve();
    };
    socket.onerror = () => {
      wsOpen = false;
    };
    socket.onclose = () => {
      wsOpen = false;
    };
    setTimeout(() => {
      if (!wsOpen) reject(new Error(`websocket to ${WS_URL} did not open`));
    }, 5000);
  });
}

async function stopCapture() {
  wsOpen = false;
  if (socket) {
    try {
      socket.close();
    } catch {}
    socket = null;
  }
  if (context && context.worklet) {
    try {
      context.worklet.disconnect();
    } catch {}
    context = null;
  }
  if (sourceNode) {
    try {
      sourceNode.disconnect();
    } catch {}
    sourceNode = null;
  }
  if (audioContext) {
    try {
      await audioContext.close();
    } catch {}
    audioContext = null;
  }
  if (stream) {
    try {
      stream.getTracks().forEach((track) => track.stop());
    } catch {}
    stream = null;
  }
}
