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

  const useUserGesture = true; // getDisplayMedia not used; tabCapture stream works directly
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

  audioContext = new AudioContext({ sampleRate: 48000 });
  sourceNode = audioContext.createMediaStreamSource(stream);
  await audioContext.audioWorklet.addModule("audio-processor.js");
  const worklet = new AudioWorkletNode(audioContext, "pcm-processor", {
    numberOfInputs: 1,
    numberOfOutputs: 1,
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
  worklet.connect(audioContext.destination); // keep the graph alive

  context = { worklet };
  connectSocket();
}

function connectSocket() {
  socket = new WebSocket(WS_URL);
  socket.binaryType = "arraybuffer";
  socket.onopen = () => {
    wsOpen = true;
  };
  socket.onclose = () => {
    wsOpen = false;
  };
  socket.onerror = () => {
    wsOpen = false;
  };
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
