/**
 * AudioWorklet processor for audio playback
 * Runs on the audio rendering thread (NOT main thread)
 *
 * Receives pre-decoded Float32 audio samples via transferable buffer.
 * Main thread handles base64 decode (fast), we handle audio output timing.
 *
 * Uses efficient circular buffer with Float32Array - no push/shift/splice.
 */

class PlaybackProcessor extends AudioWorkletProcessor {
  constructor() {
    super();

    // Circular buffer - 2 seconds at actual sample rate
    // sampleRate is available in AudioWorkletGlobalScope
    const bufferDuration = 2; // seconds
    this.bufferSize = Math.floor(sampleRate * bufferDuration);
    this.buffer = new Float32Array(this.bufferSize);
    this.writeIndex = 0;
    this.readIndex = 0;
    this.samplesAvailable = 0;

    this.muted = false;
    this.volume = 1.0;

    // Listen for audio data from main thread
    this.port.onmessage = (event) => {
      const { type, samples, muted, volume } = event.data;

      if (type === 'audio') {
        // samples is already Float32Array (transferred from main thread)
        this.writeToBuffer(samples);
      } else if (type === 'mute') {
        this.muted = muted;
      } else if (type === 'volume') {
        this.volume = Math.max(0, Math.min(1, volume));
      }
    };
  }

  /**
   * Write samples to circular buffer (O(1) amortized)
   */
  writeToBuffer(samples) {
    const len = samples.length;

    for (let i = 0; i < len; i++) {
      // If buffer is full, drop oldest sample (overwrite)
      if (this.samplesAvailable >= this.bufferSize) {
        this.readIndex = (this.readIndex + 1) % this.bufferSize;
        this.samplesAvailable--;
      }

      this.buffer[this.writeIndex] = samples[i];
      this.writeIndex = (this.writeIndex + 1) % this.bufferSize;
      this.samplesAvailable++;
    }
  }

  /**
   * Read one sample from circular buffer (O(1))
   */
  readFromBuffer() {
    if (this.samplesAvailable === 0) {
      return 0; // Silence when no data
    }

    const sample = this.buffer[this.readIndex];
    this.readIndex = (this.readIndex + 1) % this.bufferSize;
    this.samplesAvailable--;
    return sample;
  }

  process(inputs, outputs, parameters) {
    const output = outputs[0];
    if (!output || output.length === 0) return true;

    const channel = output[0];
    const samplesToWrite = channel.length; // Usually 128 samples

    // Fill output buffer from our circular buffer
    for (let i = 0; i < samplesToWrite; i++) {
      if (this.muted) {
        channel[i] = 0;
      } else {
        channel[i] = this.readFromBuffer() * this.volume;
      }
    }

    return true; // Keep processor alive
  }
}

registerProcessor('playback-processor', PlaybackProcessor);
