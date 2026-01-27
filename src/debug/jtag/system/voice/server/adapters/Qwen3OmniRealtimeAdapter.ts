/**
 * Qwen3OmniRealtimeAdapter - Audio-native adapter for Alibaba's Qwen3-Omni
 *
 * Qwen3-Omni is an open-source, natively multimodal model that:
 * - Processes audio input directly (no STT needed)
 * - Generates audio output directly (no TTS needed)
 * - Supports 10 languages, 49 voices
 * - Uses WebSocket protocol compatible with OpenAI Realtime API
 *
 * @see https://github.com/QwenLM/Qwen3-Omni
 * @see https://www.alibabacloud.com/help/en/model-studio/realtime
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

const DASHSCOPE_API_KEY = process.env.DASHSCOPE_API_KEY || process.env.QWEN_API_KEY;

/**
 * Qwen3-Omni Realtime Adapter
 *
 * Usage:
 *   const adapter = new Qwen3OmniRealtimeAdapter();
 *   await adapter.connect({ voice: 'Cherry' });
 *   adapter.onAudioOutput((samples) => playAudio(samples));
 *   adapter.sendAudio(micSamples);
 */
export class Qwen3OmniRealtimeAdapter implements IAudioNativeAdapter {
  readonly providerId = 'alibaba';
  readonly modelId = 'qwen3-omni-flash-realtime';

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
    this.apiKey = apiKey || DASHSCOPE_API_KEY;
  }

  /**
   * Connect to Qwen3-Omni Realtime API
   */
  async connect(config: Partial<AudioNativeSessionConfig> = {}): Promise<void> {
    if (!this.apiKey) {
      throw new Error('DASHSCOPE_API_KEY or QWEN_API_KEY environment variable required');
    }

    this.sessionConfig = { ...DEFAULT_AUDIO_NATIVE_CONFIG, ...config };

    const endpoint = `${AUDIO_NATIVE_ENDPOINTS['qwen3-omni-flash-realtime']}?model=${this.modelId}`;

    return new Promise((resolve, reject) => {
      this.ws = new WebSocket(endpoint, {
        headers: {
          'Authorization': `Bearer ${this.apiKey}`,
          'OpenAI-Beta': 'realtime=v1',  // Required header for DashScope realtime API
        },
      });

      this.ws.on('open', () => {
        console.log(`🔊 Qwen3-Omni: Connected to ${this.modelId}`);

        // Send session configuration
        this.sendEvent({
          type: 'session.update',
          event_id: this.nextEventId(),
          session: {
            modalities: this.sessionConfig!.modalities,
            voice: config.voice || 'Cherry',
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
        console.error('🔊 Qwen3-Omni: WebSocket error:', error);
        this.emitError(error);
        reject(error);
      });

      this.ws.on('close', (code, reason) => {
        console.log(`🔊 Qwen3-Omni: Disconnected (code: ${code}, reason: ${reason})`);
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
      console.warn('🔊 Qwen3-Omni: Cannot send audio - not connected');
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
      console.error('🔊 Qwen3-Omni: Failed to parse message:', error);
    }
  }

  private handleEvent(event: AudioNativeServerEvent): void {
    switch (event.type) {
      case 'session.created':
      case 'session.updated':
        console.log(`🔊 Qwen3-Omni: Session ${event.type.split('.')[1]}`);
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
        console.log(`🔊 Qwen3-Omni: Response completed`);
        break;

      case 'error':
        this.handleError(event as ErrorEvent);
        break;

      default:
        // Ignore other events
        break;
    }
  }

  private handleSpeechStarted(event: InputAudioBufferSpeechStartedEvent): void {
    console.log(`🔊 Qwen3-Omni: Speech started at ${event.audio_start_ms}ms`);
    for (const callback of this.speechDetectedCallbacks) {
      callback(true);
    }
  }

  private handleSpeechStopped(event: InputAudioBufferSpeechStoppedEvent): void {
    console.log(`🔊 Qwen3-Omni: Speech stopped at ${event.audio_end_ms}ms`);
    for (const callback of this.speechDetectedCallbacks) {
      callback(false);
    }
  }

  private handleAudioDelta(event: ResponseAudioDeltaEvent): void {
    // Decode base64 to Int16Array
    const buffer = Buffer.from(event.delta, 'base64');

    // Output is 24kHz PCM, convert to Int16Array
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
    console.log(`🔊 Qwen3-Omni: Transcript: "${finalTranscript.slice(0, 50)}..."`);

    // Emit final transcript
    for (const callback of this.transcriptCallbacks) {
      callback(finalTranscript, true);
    }

    this.transcriptBuffer = '';
  }

  private handleError(event: ErrorEvent): void {
    const error = new Error(`${event.error.code}: ${event.error.message}`);
    console.error('🔊 Qwen3-Omni: Error:', error.message);
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
export function createQwen3OmniAdapter(apiKey?: string): IAudioNativeAdapter {
  return new Qwen3OmniRealtimeAdapter(apiKey);
}
