// AudioWorkletProcessor: converts incoming 48 kHz multi-channel audio to
// 16 kHz mono f32 and posts raw little-endian byte chunks to the main thread
// for the WebSocket.

class PcmProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const opts = options.processorOptions || {};
    this.targetRate = opts.targetRate || 16000;
    this.sourceRate = opts.sampleRate || 48000;
    this.channels = opts.channels || 1;
    this.ratio = this.sourceRate / this.targetRate;
    this.samples = 0; // consumed target-rate samples
    this.buffer = []; // mono target-rate samples awaiting a 20 ms chunk
  }

  process(inputs, _outputs) {
    const input = inputs[0];
    if (!input || input.length === 0) {
      return true;
    }
    const frames = input[0].length;
    const channelData = [];
    for (let i = 0; i < Math.min(this.channels, input.length); i++) {
      channelData.push(input[i]);
    }
    if (channelData.length === 0) return true;

    for (let i = 0; i < frames; i++) {
      const sourceIndex = Math.floor(i / this.ratio);
      let sample = 0;
      for (const channel of channelData) {
        sample += channel[sourceIndex];
      }
      sample /= channelData.length;
      this.buffer.push(sample);
    }

    // 20 ms at 16 kHz = 320 samples.
    const CHUNK = 320;
    while (this.buffer.length >= CHUNK) {
      const chunk = this.buffer.splice(0, CHUNK);
      const bytes = new Float32Array(chunk);
      this.port.postMessage(bytes.buffer, [bytes.buffer]);
    }
    return true;
  }
}

registerProcessor("pcm-processor", PcmProcessor);
