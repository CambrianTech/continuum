/**
 * AudioWorklet processor for microphone capture
 * Runs on the audio rendering thread (NOT main thread)
 *
 * Captures 128-sample chunks and batches them into frames
 * before sending to main thread via postMessage.
 * Also calculates RMS audio level for visual feedback.
 */

class MicrophoneProcessor extends AudioWorkletProcessor {
  constructor() {
    super();

    // Efficient circular buffer for batching
    this.frameSize = 512; // Batch 4x 128-sample chunks into 512-sample frames
    this.buffer = new Float32Array(this.frameSize);
    this.writeIndex = 0;
  }

  /**
   * Calculate RMS (root mean square) level of samples
   * Returns 0.0 to 1.0
   */
  calculateRMS(samples) {
    let sum = 0;
    for (let i = 0; i < samples.length; i++) {
      sum += samples[i] * samples[i];
    }
    return Math.sqrt(sum / samples.length);
  }

  process(inputs, outputs, parameters) {
    const input = inputs[0];
    if (!input || input.length === 0) return true;

    const channelData = input[0];
    if (!channelData) return true;

    // Copy samples to our buffer
    for (let i = 0; i < channelData.length; i++) {
      this.buffer[this.writeIndex] = channelData[i];
      this.writeIndex++;

      // When buffer is full, send frame with level
      if (this.writeIndex >= this.frameSize) {
        // Calculate RMS level before sending
        const level = this.calculateRMS(this.buffer);

        // Create a copy for transfer (we need to keep our buffer)
        const frame = new Float32Array(this.buffer);

        // Send frame with level metadata
        // Transfer ownership of frame buffer (zero-copy)
        this.port.postMessage({ frame, level }, [frame.buffer]);

        // Reset buffer index
        this.writeIndex = 0;
      }
    }

    return true; // Keep processor alive
  }
}

registerProcessor('microphone-processor', MicrophoneProcessor);
