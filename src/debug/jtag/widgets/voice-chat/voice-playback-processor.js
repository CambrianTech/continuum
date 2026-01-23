/**
 * Voice Playback AudioWorklet Processor
 *
 * Runs in the audio rendering thread for smooth audio playback.
 * Implements a jitter buffer to handle network timing variations.
 *
 * Input: Int16 PCM at 16kHz from WebSocket
 * Output: Float32 PCM at system rate (typically 48kHz)
 */

class VoicePlaybackProcessor extends AudioWorkletProcessor {
  constructor() {
    super();

    // Jitter buffer - holds audio chunks for smooth playback
    // Each chunk is 320 samples (20ms at 16kHz)
    this.jitterBuffer = [];
    this.maxBufferSize = 10; // 200ms max buffer
    this.minBufferSize = 2;  // 40ms min before playback starts

    // Resampling state
    this.inputSampleRate = 16000;
    this.outputSampleRate = 48000; // Will be set from sampleRate
    this.resampleRatio = this.outputSampleRate / this.inputSampleRate;

    // Playback state
    this.isPlaying = false;
    this.currentChunk = null;
    this.currentChunkIndex = 0;

    // Underrun detection
    this.underrunCount = 0;

    // Handle messages from main thread
    this.port.onmessage = (event) => {
      if (event.data.type === 'audio') {
        this.receiveAudio(event.data.samples);
      } else if (event.data.type === 'clear') {
        this.clearBuffer();
      } else if (event.data.type === 'setSampleRate') {
        this.outputSampleRate = event.data.sampleRate;
        this.resampleRatio = this.outputSampleRate / this.inputSampleRate;
      }
    };
  }

  /**
   * Receive audio data from main thread
   */
  receiveAudio(arrayBuffer) {
    const int16Data = new Int16Array(arrayBuffer);
    const float32Data = this.int16ToFloat(int16Data);

    // Add to jitter buffer
    if (this.jitterBuffer.length < this.maxBufferSize) {
      this.jitterBuffer.push(float32Data);
    } else {
      // Buffer full - drop oldest
      this.jitterBuffer.shift();
      this.jitterBuffer.push(float32Data);
      this.port.postMessage({ type: 'bufferOverflow' });
    }

    // Start playback when buffer is ready
    if (!this.isPlaying && this.jitterBuffer.length >= this.minBufferSize) {
      this.isPlaying = true;
      this.port.postMessage({ type: 'playbackStart' });
    }
  }

  /**
   * Clear the jitter buffer (for interruption/barge-in)
   */
  clearBuffer() {
    this.jitterBuffer = [];
    this.currentChunk = null;
    this.currentChunkIndex = 0;
    this.isPlaying = false;
    this.port.postMessage({ type: 'playbackStop' });
  }

  /**
   * Convert Int16 [-32768, 32767] to Float32 [-1, 1]
   */
  int16ToFloat(int16Array) {
    const float32Array = new Float32Array(int16Array.length);
    for (let i = 0; i < int16Array.length; i++) {
      float32Array[i] = int16Array[i] / 32768.0;
    }
    return float32Array;
  }

  /**
   * Upsample from 16kHz to output rate
   * Linear interpolation
   */
  upsample(input, outputLength) {
    const output = new Float32Array(outputLength);
    const ratio = input.length / outputLength;

    for (let i = 0; i < outputLength; i++) {
      const srcIndex = i * ratio;
      const srcIndexFloor = Math.floor(srcIndex);
      const fraction = srcIndex - srcIndexFloor;

      if (srcIndexFloor + 1 < input.length) {
        output[i] = input[srcIndexFloor] * (1 - fraction) +
                    input[srcIndexFloor + 1] * fraction;
      } else if (srcIndexFloor < input.length) {
        output[i] = input[srcIndexFloor];
      }
    }

    return output;
  }

  /**
   * Get next sample from buffer with resampling
   */
  getNextSamples(count) {
    const output = new Float32Array(count);
    let outputIndex = 0;

    while (outputIndex < count) {
      // Need new chunk?
      if (!this.currentChunk || this.currentChunkIndex >= this.currentChunk.length) {
        if (this.jitterBuffer.length > 0) {
          this.currentChunk = this.jitterBuffer.shift();
          this.currentChunkIndex = 0;
        } else {
          // Buffer underrun - output silence
          this.underrunCount++;
          if (this.isPlaying) {
            this.isPlaying = false;
            this.port.postMessage({ type: 'bufferUnderrun', count: this.underrunCount });
          }
          // Fill rest with silence
          while (outputIndex < count) {
            output[outputIndex++] = 0;
          }
          return output;
        }
      }

      // How many input samples do we need for remaining output?
      const outputRemaining = count - outputIndex;
      const inputAvailable = this.currentChunk.length - this.currentChunkIndex;
      const inputNeeded = Math.ceil(outputRemaining / this.resampleRatio);
      const inputToUse = Math.min(inputAvailable, inputNeeded);

      // Get input slice
      const inputSlice = this.currentChunk.subarray(
        this.currentChunkIndex,
        this.currentChunkIndex + inputToUse
      );

      // Upsample
      const outputSamples = Math.min(
        Math.floor(inputToUse * this.resampleRatio),
        outputRemaining
      );
      const upsampled = this.upsample(inputSlice, outputSamples);

      // Copy to output
      output.set(upsampled, outputIndex);
      outputIndex += outputSamples;
      this.currentChunkIndex += inputToUse;
    }

    return output;
  }

  /**
   * Process audio - called every 128 samples
   */
  process(inputs, outputs, parameters) {
    const output = outputs[0];
    if (!output || !output[0]) return true;

    const channel = output[0];

    if (this.isPlaying || this.jitterBuffer.length > 0) {
      const samples = this.getNextSamples(channel.length);
      channel.set(samples);
    } else {
      // Silence when not playing
      channel.fill(0);
    }

    return true; // Keep processor alive
  }
}

registerProcessor('voice-playback-processor', VoicePlaybackProcessor);
