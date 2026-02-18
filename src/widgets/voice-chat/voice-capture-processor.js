/**
 * Voice Capture AudioWorklet Processor
 *
 * Runs in the audio rendering thread for low-latency mic capture.
 * Collects 20ms chunks of audio and sends to main thread.
 *
 * Input: Float32 PCM from getUserMedia (typically 48kHz)
 * Output: Int16 PCM at 16kHz (downsampled for speech)
 */

class VoiceCaptureProcessor extends AudioWorkletProcessor {
  constructor() {
    super();

    // Buffer to accumulate samples before sending
    // At 48kHz, 20ms = 960 samples
    // At 16kHz (target), 20ms = 320 samples
    this.buffer = new Float32Array(960);
    this.bufferIndex = 0;

    // Resampling state
    this.inputSampleRate = 48000; // Will be set from options
    this.outputSampleRate = 16000;
    this.resampleRatio = this.inputSampleRate / this.outputSampleRate;

    // Simple voice activity detection
    this.vadThreshold = 0.01;
    this.silenceFrames = 0;
    this.maxSilenceFrames = 50; // ~1 second of silence
    this.isSpeaking = false;

    // Handle messages from main thread
    this.port.onmessage = (event) => {
      if (event.data.type === 'setSampleRate') {
        this.inputSampleRate = event.data.sampleRate;
        this.resampleRatio = this.inputSampleRate / this.outputSampleRate;
      } else if (event.data.type === 'setVadThreshold') {
        this.vadThreshold = event.data.threshold;
      }
    };
  }

  /**
   * Calculate RMS (root mean square) for voice activity detection
   */
  calculateRMS(samples) {
    let sum = 0;
    for (let i = 0; i < samples.length; i++) {
      sum += samples[i] * samples[i];
    }
    return Math.sqrt(sum / samples.length);
  }

  /**
   * Downsample from input rate to 16kHz
   * Simple linear interpolation - good enough for speech
   */
  downsample(input) {
    const outputLength = Math.floor(input.length / this.resampleRatio);
    const output = new Float32Array(outputLength);

    for (let i = 0; i < outputLength; i++) {
      const srcIndex = i * this.resampleRatio;
      const srcIndexFloor = Math.floor(srcIndex);
      const fraction = srcIndex - srcIndexFloor;

      if (srcIndexFloor + 1 < input.length) {
        // Linear interpolation
        output[i] = input[srcIndexFloor] * (1 - fraction) +
                    input[srcIndexFloor + 1] * fraction;
      } else {
        output[i] = input[srcIndexFloor];
      }
    }

    return output;
  }

  /**
   * Convert Float32 [-1, 1] to Int16 [-32768, 32767]
   */
  floatToInt16(float32Array) {
    const int16Array = new Int16Array(float32Array.length);
    for (let i = 0; i < float32Array.length; i++) {
      // Clamp and convert
      const s = Math.max(-1, Math.min(1, float32Array[i]));
      int16Array[i] = s < 0 ? s * 0x8000 : s * 0x7FFF;
    }
    return int16Array;
  }

  /**
   * Process audio - called every 128 samples (~2.67ms at 48kHz)
   */
  process(inputs, outputs, parameters) {
    const input = inputs[0];
    if (!input || !input[0]) return true;

    const samples = input[0]; // Mono channel

    // Accumulate samples
    for (let i = 0; i < samples.length; i++) {
      this.buffer[this.bufferIndex++] = samples[i];

      // When buffer is full (20ms), process and send
      if (this.bufferIndex >= this.buffer.length) {
        this.processBuffer();
        this.bufferIndex = 0;
      }
    }

    return true; // Keep processor alive
  }

  /**
   * Process a complete 20ms buffer
   */
  processBuffer() {
    // Calculate audio level for VAD
    const rms = this.calculateRMS(this.buffer);
    const isSpeechFrame = rms > this.vadThreshold;

    // Update speaking state with hysteresis
    if (isSpeechFrame) {
      this.silenceFrames = 0;
      if (!this.isSpeaking) {
        this.isSpeaking = true;
        this.port.postMessage({ type: 'vadStart' });
      }
    } else {
      this.silenceFrames++;
      if (this.isSpeaking && this.silenceFrames > this.maxSilenceFrames) {
        this.isSpeaking = false;
        this.port.postMessage({ type: 'vadEnd' });
      }
    }

    // Downsample to 16kHz
    const downsampled = this.downsample(this.buffer);

    // Convert to Int16
    const int16Data = this.floatToInt16(downsampled);

    // Send to main thread
    this.port.postMessage({
      type: 'audio',
      samples: int16Data.buffer,
      level: rms,
      isSpeaking: this.isSpeaking
    }, [int16Data.buffer]); // Transfer ownership for zero-copy
  }
}

registerProcessor('voice-capture-processor', VoiceCaptureProcessor);
