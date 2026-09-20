/**
 * AudioNativeTypes - Types for audio-native AI models
 *
 * These models can hear raw audio and speak without STT/TTS pipeline:
 * - OpenAI gpt-realtime
 * - Google Gemini 2.5 Flash
 * - Alibaba Qwen3-Omni
 * - Amazon Nova Sonic
 *
 * Protocol based on OpenAI Realtime API (Qwen3-Omni uses same format)
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';

/**
 * Audio format configuration
 */
export interface AudioFormat {
  sampleRate: number;      // 16000 for input, 24000 for output
  channels: number;        // 1 (mono)
  bitDepth: number;        // 16
  encoding: 'pcm16' | 'pcm24';
}

/**
 * Session configuration for audio-native models
 */
export interface AudioNativeSessionConfig {
  modalities: ('text' | 'audio')[];
  voice?: string;                    // Voice ID (e.g., "Cherry", "Ethan")
  inputAudioFormat: AudioFormat;
  outputAudioFormat: AudioFormat;
  turnDetection: {
    type: 'server_vad' | 'none';
    threshold?: number;              // VAD threshold (0-1)
    prefixPaddingMs?: number;
    silenceDurationMs?: number;
  };
  instructions?: string;             // System prompt
}

/**
 * Events sent TO the audio-native model
 */
export type AudioNativeClientEvent =
  | SessionUpdateEvent
  | InputAudioBufferAppendEvent
  | InputAudioBufferCommitEvent
  | ResponseCreateEvent
  | ResponseCancelEvent;

export interface SessionUpdateEvent {
  type: 'session.update';
  event_id: string;
  session: Partial<AudioNativeSessionConfig>;
}

export interface InputAudioBufferAppendEvent {
  type: 'input_audio_buffer.append';
  event_id: string;
  audio: string;  // Base64 encoded PCM16
}

export interface InputAudioBufferCommitEvent {
  type: 'input_audio_buffer.commit';
  event_id: string;
}

export interface ResponseCreateEvent {
  type: 'response.create';
  event_id: string;
  response?: {
    modalities?: ('text' | 'audio')[];
  };
}

export interface ResponseCancelEvent {
  type: 'response.cancel';
  event_id: string;
}

/**
 * Events received FROM the audio-native model
 */
export type AudioNativeServerEvent =
  | SessionCreatedEvent
  | SessionUpdatedEvent
  | InputAudioBufferSpeechStartedEvent
  | InputAudioBufferSpeechStoppedEvent
  | ResponseCreatedEvent
  | ResponseAudioDeltaEvent
  | ResponseAudioDoneEvent
  | ResponseAudioTranscriptDeltaEvent
  | ResponseAudioTranscriptDoneEvent
  | ResponseDoneEvent
  | ErrorEvent;

export interface SessionCreatedEvent {
  type: 'session.created';
  event_id: string;
  session: AudioNativeSessionConfig;
}

export interface SessionUpdatedEvent {
  type: 'session.updated';
  event_id: string;
  session: AudioNativeSessionConfig;
}

export interface InputAudioBufferSpeechStartedEvent {
  type: 'input_audio_buffer.speech_started';
  event_id: string;
  audio_start_ms: number;
}

export interface InputAudioBufferSpeechStoppedEvent {
  type: 'input_audio_buffer.speech_stopped';
  event_id: string;
  audio_end_ms: number;
}

export interface ResponseCreatedEvent {
  type: 'response.created';
  event_id: string;
  response: {
    id: string;
    status: 'in_progress' | 'completed' | 'cancelled';
  };
}

export interface ResponseAudioDeltaEvent {
  type: 'response.audio.delta';
  event_id: string;
  response_id: string;
  delta: string;  // Base64 encoded audio chunk
}

export interface ResponseAudioDoneEvent {
  type: 'response.audio.done';
  event_id: string;
  response_id: string;
}

export interface ResponseAudioTranscriptDeltaEvent {
  type: 'response.audio_transcript.delta';
  event_id: string;
  response_id: string;
  delta: string;  // Text transcript chunk
}

export interface ResponseAudioTranscriptDoneEvent {
  type: 'response.audio_transcript.done';
  event_id: string;
  response_id: string;
  transcript: string;  // Full transcript
}

export interface ResponseDoneEvent {
  type: 'response.done';
  event_id: string;
  response: {
    id: string;
    status: 'completed' | 'cancelled' | 'failed';
  };
}

export interface ErrorEvent {
  type: 'error';
  event_id: string;
  error: {
    type: string;
    code: string;
    message: string;
  };
}

/**
 * Audio-native model connection state
 */
export interface AudioNativeConnection {
  userId: UUID;
  displayName: string;
  modelId: string;
  callId: string;
  isConnected: boolean;
  sessionConfig?: AudioNativeSessionConfig;
}

/**
 * Audio-native adapter interface
 */
export interface IAudioNativeAdapter {
  readonly providerId: string;
  readonly modelId: string;

  /**
   * Connect to the audio-native model's realtime endpoint
   */
  connect(config: AudioNativeSessionConfig): Promise<void>;

  /**
   * Disconnect from the model
   */
  disconnect(): Promise<void>;

  /**
   * Send audio chunk to the model
   * @param samples - Int16Array of PCM samples (16kHz mono)
   */
  sendAudio(samples: Int16Array): void;

  /**
   * Cancel current response (for interruptions)
   */
  cancelResponse(): void;

  /**
   * Check if connected
   */
  isConnected(): boolean;

  /**
   * Subscribe to audio output
   */
  onAudioOutput(callback: (samples: Int16Array) => void): void;

  /**
   * Subscribe to transcript output
   */
  onTranscript(callback: (text: string, isFinal: boolean) => void): void;

  /**
   * Subscribe to speech detection events
   */
  onSpeechDetected(callback: (started: boolean) => void): void;

  /**
   * Subscribe to errors
   */
  onError(callback: (error: Error) => void): void;
}

/**
 * Available audio-native voices by provider
 */
export const AUDIO_NATIVE_VOICES = {
  'qwen3-omni': [
    'Cherry', 'Serena', 'Ethan', 'Chelsie', 'Aura',
    // ... 49 total voices for qwen3-omni-flash-realtime-2025-12-01
  ],
  'gpt-realtime': [
    'alloy', 'echo', 'fable', 'onyx', 'nova', 'shimmer',
  ],
  'gemini-live': [
    'Puck', 'Charon', 'Kore', 'Fenrir', 'Aoede',
  ],
} as const;

/**
 * Audio-native model endpoints
 */
export const AUDIO_NATIVE_ENDPOINTS = {
  'qwen3-omni-flash-realtime': 'wss://dashscope-intl.aliyuncs.com/api-ws/v1/realtime',
  'gpt-4o-realtime': 'wss://api.openai.com/v1/realtime',
  'gemini-2.0-flash-live': 'wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1alpha.GenerativeService.BidiGenerateContent',
} as const;

/**
 * Default session config for audio conversations
 */
export const DEFAULT_AUDIO_NATIVE_CONFIG: AudioNativeSessionConfig = {
  modalities: ['text', 'audio'],
  inputAudioFormat: {
    sampleRate: 16000,
    channels: 1,
    bitDepth: 16,
    encoding: 'pcm16',
  },
  outputAudioFormat: {
    sampleRate: 24000,
    channels: 1,
    bitDepth: 16,
    encoding: 'pcm24',
  },
  turnDetection: {
    type: 'server_vad',
    threshold: 0.5,
    silenceDurationMs: 500,
  },
};
