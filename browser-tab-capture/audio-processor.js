// AudioWorkletProcessor: converts incoming 48 kHz multi-channel audio to
// 16 kHz mono f32 and posts raw little-endian byte chunks to the main thread
// for the WebSocket.
//
// The downmix uses a running accumulator so exactly sourceRate/targetRate
// input frames produce one output sample, with inter-channel averaging. This
// keeps the output timeline aligned with the source.

class PcmProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const opts = options.processorOptions || {};
    this.targetRate = opts.targetRate || 16000;
    this.sourceRate = opts.sampleRate || 48000;
    this.channels = opts.channels || 1;
    this.ratio = this.sourceRate / this.targetRate;
    this.acc = 0; // fractional input-frame position
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

    // Step through input frames, emitting one output sample every `ratio`
    // frames, accumulated across the block boundary.
    for (let i = 0; i < frames; i++) {
      let mono = 0;
      for (const channel of channelData) {
        mono += channel[i];
      }
      mono /= channelData.length;
      this.acc += 1;
      if (this.acc >= this.ratio) {
        this.buffer.push(mono);
        this.acc -= this.ratio;
      }
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
