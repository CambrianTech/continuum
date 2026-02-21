/**
 * GPT4oRealtimeAdapter - Audio-native adapter for OpenAI's GPT-4o Realtime API
 *
 * GPT-4o Realtime provides:
 * - Direct audio input (no STT needed)
 * - Direct audio output (no TTS needed)
 * - 6 voices (alloy, echo, fable, onyx, nova, shimmer)
 * - WebSocket protocol (OpenAI Realtime API — same format Qwen3-Omni cloned)
 *
 * @see https://platform.openai.com/docs/guides/realtime
 */

import WebSocket from 'ws';
import type {
  IAudioNativeAdapter,
  AudioNativeSessionConfig,
  AudioNativeClientEvent,
  AudioNativeServerEvent,
  ResponseAudioDeltaEvent,
  ResponseAudioTranscriptDeltaEvent,
  ResponseAudioTranscriptDoneEvent,
  InputAudioBufferSpeechStartedEvent,
  InputAudioBufferSpeechStoppedEvent,
  ErrorEvent,
} from '../../shared/AudioNativeTypes';
import {
  DEFAULT_AUDIO_NATIVE_CONFIG,
  AUDIO_NATIVE_ENDPOINTS,
} from '../../shared/AudioNativeTypes';

const OPENAI_API_KEY = process.env.OPENAI_API_KEY;

/**
 * GPT-4o Realtime Adapter
 *
 * Usage:
 *   const adapter = new GPT4oRealtimeAdapter();
 *   await adapter.connect({ voice: 'alloy' });
 *   adapter.onAudioOutput((samples) => playAudio(samples));
 *   adapter.sendAudio(micSamples);
 */
export class GPT4oRealtimeAdapter implements IAudioNativeAdapter {
  readonly providerId = 'openai';
  readonly modelId = 'gpt-4o-realtime-preview-2024-12-17';

  private ws: WebSocket | null = null;
  private sessionConfig: AudioNativeSessionConfig | null = null;
  private eventCounter = 0;

  // Callbacks
  private audioOutputCallbacks: ((samples: Int16Array) => void)[] = [];
  private transcriptCallbacks: ((text: string, isFinal: boolean) => void)[] = [];
  private speechDetectedCallbacks: ((started: boolean) => void)[] = [];
  private errorCallbacks: ((error: Error) => void)[] = [];

  // Audio buffer for assembling output
  private outputAudioBuffer: Int16Array[] = [];
  private transcriptBuffer = '';

  constructor(private apiKey?: string) {
    this.apiKey = apiKey || OPENAI_API_KEY;
  }

  /**
   * Connect to GPT-4o Realtime API
   */
  async connect(config: Partial<AudioNativeSessionConfig> = {}): Promise<void> {
    if (!this.apiKey) {
      throw new Error('OPENAI_API_KEY environment variable required');
    }

    this.sessionConfig = { ...DEFAULT_AUDIO_NATIVE_CONFIG, ...config };

    const endpoint = `${AUDIO_NATIVE_ENDPOINTS['gpt-4o-realtime']}?model=${this.modelId}`;

    return new Promise((resolve, reject) => {
      this.ws = new WebSocket(endpoint, {
        headers: {
          'Authorization': `Bearer ${this.apiKey}`,
          'OpenAI-Beta': 'realtime=v1',
        },
      });

      this.ws.on('open', () => {
        console.log(`🔊 GPT-4o Realtime: Connected to ${this.modelId}`);

        // Send session configuration
        this.sendEvent({
          type: 'session.update',
          event_id: this.nextEventId(),
          session: {
            modalities: this.sessionConfig!.modalities,
            voice: config.voice || 'alloy',
            inputAudioFormat: this.sessionConfig!.inputAudioFormat,
            outputAudioFormat: this.sessionConfig!.outputAudioFormat,
            turnDetection: this.sessionConfig!.turnDetection,
            instructions: config.instructions,
          },
        });

        resolve();
      });

      this.ws.on('message', (data: WebSocket.Data) => {
        this.handleMessage(data);
      });

      this.ws.on('error', (error) => {
        console.error('🔊 GPT-4o Realtime: WebSocket error:', error);
        this.emitError(error);
        reject(error);
      });

      this.ws.on('close', (code, reason) => {
        console.log(`🔊 GPT-4o Realtime: Disconnected (code: ${code}, reason: ${reason})`);
        this.ws = null;
      });
    });
  }

  /**
   * Disconnect from the model
   */
  async disconnect(): Promise<void> {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.close(1000, 'Client disconnect');
    }
    this.ws = null;
    this.sessionConfig = null;
  }

  /**
   * Send audio chunk to the model
   * @param samples - Int16Array of PCM samples (16kHz mono)
   */
  sendAudio(samples: Int16Array): void {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      console.warn('🔊 GPT-4o Realtime: Cannot send audio - not connected');
      return;
    }

    // Convert Int16Array to base64
    const buffer = Buffer.from(samples.buffer, samples.byteOffset, samples.byteLength);
    const base64Audio = buffer.toString('base64');

    this.sendEvent({
      type: 'input_audio_buffer.append',
      event_id: this.nextEventId(),
      audio: base64Audio,
    });
  }

  /**
   * Cancel current response (for interruptions)
   */
  cancelResponse(): void {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return;
    }

    this.sendEvent({
      type: 'response.cancel',
      event_id: this.nextEventId(),
    });

    // Clear buffers
    this.outputAudioBuffer = [];
    this.transcriptBuffer = '';
  }

  /**
   * Check if connected
   */
  isConnected(): boolean {
    return this.ws !== null && this.ws.readyState === WebSocket.OPEN;
  }

  /**
   * Subscribe to audio output
   */
  onAudioOutput(callback: (samples: Int16Array) => void): void {
    this.audioOutputCallbacks.push(callback);
  }

  /**
   * Subscribe to transcript output
   */
  onTranscript(callback: (text: string, isFinal: boolean) => void): void {
    this.transcriptCallbacks.push(callback);
  }

  /**
   * Subscribe to speech detection events
   */
  onSpeechDetected(callback: (started: boolean) => void): void {
    this.speechDetectedCallbacks.push(callback);
  }

  /**
   * Subscribe to errors
   */
  onError(callback: (error: Error) => void): void {
    this.errorCallbacks.push(callback);
  }

  // ============================================================================
  // Private Methods
  // ============================================================================

  private sendEvent(event: AudioNativeClientEvent): void {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(event));
    }
  }

  private nextEventId(): string {
    return `evt_${++this.eventCounter}`;
  }

  private handleMessage(data: WebSocket.Data): void {
    try {
      const event = JSON.parse(data.toString()) as AudioNativeServerEvent;
      this.handleEvent(event);
    } catch (error) {
      console.error('🔊 GPT-4o Realtime: Failed to parse message:', error);
    }
  }

  private handleEvent(event: AudioNativeServerEvent): void {
    switch (event.type) {
      case 'session.created':
      case 'session.updated':
        console.log(`🔊 GPT-4o Realtime: Session ${event.type.split('.')[1]}`);
        break;

      case 'input_audio_buffer.speech_started':
        this.handleSpeechStarted(event as InputAudioBufferSpeechStartedEvent);
        break;

      case 'input_audio_buffer.speech_stopped':
        this.handleSpeechStopped(event as InputAudioBufferSpeechStoppedEvent);
        break;

      case 'response.audio.delta':
        this.handleAudioDelta(event as ResponseAudioDeltaEvent);
        break;

      case 'response.audio.done':
        this.flushAudioBuffer();
        break;

      case 'response.audio_transcript.delta':
        this.handleTranscriptDelta(event as ResponseAudioTranscriptDeltaEvent);
        break;

      case 'response.audio_transcript.done':
        this.handleTranscriptDone(event as ResponseAudioTranscriptDoneEvent);
        break;

      case 'response.done':
        console.log(`🔊 GPT-4o Realtime: Response completed`);
        break;

      case 'error':
        this.handleError(event as ErrorEvent);
        break;

      default:
        // Ignore other events (rate_limits.updated, response.created, etc.)
        break;
    }
  }

  private handleSpeechStarted(event: InputAudioBufferSpeechStartedEvent): void {
    console.log(`🔊 GPT-4o Realtime: Speech started at ${event.audio_start_ms}ms`);
    for (const callback of this.speechDetectedCallbacks) {
      callback(true);
    }
  }

  private handleSpeechStopped(event: InputAudioBufferSpeechStoppedEvent): void {
    console.log(`🔊 GPT-4o Realtime: Speech stopped at ${event.audio_end_ms}ms`);
    for (const callback of this.speechDetectedCallbacks) {
      callback(false);
    }
  }

  private handleAudioDelta(event: ResponseAudioDeltaEvent): void {
    // Decode base64 to Int16Array
    const buffer = Buffer.from(event.delta, 'base64');

    // Output is 24kHz PCM16, convert to Int16Array
    const samples = new Int16Array(buffer.length / 2);
    for (let i = 0; i < samples.length; i++) {
      samples[i] = buffer.readInt16LE(i * 2);
    }

    // Stream audio immediately for low latency
    for (const callback of this.audioOutputCallbacks) {
      callback(samples);
    }

    // Also accumulate for any buffered playback needs
    this.outputAudioBuffer.push(samples);
  }

  private flushAudioBuffer(): void {
    // Audio already streamed in handleAudioDelta
    this.outputAudioBuffer = [];
  }

  private handleTranscriptDelta(event: ResponseAudioTranscriptDeltaEvent): void {
    this.transcriptBuffer += event.delta;

    // Emit partial transcript
    for (const callback of this.transcriptCallbacks) {
      callback(this.transcriptBuffer, false);
    }
  }

  private handleTranscriptDone(event: ResponseAudioTranscriptDoneEvent): void {
    const finalTranscript = event.transcript || this.transcriptBuffer;
    console.log(`🔊 GPT-4o Realtime: Transcript: "${finalTranscript.slice(0, 50)}..."`);

    // Emit final transcript
    for (const callback of this.transcriptCallbacks) {
      callback(finalTranscript, true);
    }

    this.transcriptBuffer = '';
  }

  private handleError(event: ErrorEvent): void {
    const error = new Error(`${event.error.code}: ${event.error.message}`);
    console.error('🔊 GPT-4o Realtime: Error:', error.message);
    this.emitError(error);
  }

  private emitError(error: Error): void {
    for (const callback of this.errorCallbacks) {
      callback(error);
    }
  }
}

/**
 * Factory function
 */
export function createGPT4oRealtimeAdapter(apiKey?: string): IAudioNativeAdapter {
  return new GPT4oRealtimeAdapter(apiKey);
}
