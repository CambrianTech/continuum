/**
 * AudioNativeBridge - Manages audio-native AI connections to voice calls
 *
 * ARCHITECTURE (streaming, not polling):
 * - WebSocket to call server for real-time room audio (AI hears the room)
 * - IPC to Rust for audio injection (AI speaks into the mixer)
 * - Audio-native adapters handle their own realtime API connections
 *
 * Audio flow:
 *   Room participants → call_server mixer → WebSocket binary frames → adapter.sendAudio()
 *   adapter.onAudioOutput() → resample → IPC voice/inject-audio → call_server mixer
 *
 * This mirrors AIAudioBridge but for models that natively process audio
 * (Gemini Live, Qwen3-Omni, GPT-4o Realtime) instead of text-based TTS/STT.
 */

import WebSocket from 'ws';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import type {
  IAudioNativeAdapter,
  AudioNativeConnection,
  AudioNativeSessionConfig,
} from '../shared/AudioNativeTypes';
import { DEFAULT_AUDIO_NATIVE_CONFIG, AUDIO_NATIVE_VOICES } from '../shared/AudioNativeTypes';
import { Qwen3OmniRealtimeAdapter } from './adapters/Qwen3OmniRealtimeAdapter';
import { GeminiLiveAdapter } from './adapters/GeminiLiveAdapter';
import { GPT4oRealtimeAdapter } from './adapters/GPT4oRealtimeAdapter';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../workers/continuum-core/bindings/RustCoreIPC';
import { Events } from '../../core/shared/Events';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import { EVENT_SCOPES } from '../../events/shared/EventSystemConstants';

/** Call server sample rate (single source of truth: audio_constants.rs AUDIO_SAMPLE_RATE) */
const CALL_SERVER_SAMPLE_RATE = 16000;

const STREAMING_CORE_URL = process.env.STREAMING_CORE_WS_URL || 'ws://127.0.0.1:50053';

/**
 * Binary frame protocol discriminators (must match FrameKind in types.rs)
 * First byte of every binary WebSocket message from the call server.
 */
const FRAME_KIND_AUDIO = 0x01;
const FRAME_KIND_VIDEO = 0x02;
const FRAME_KIND_AVATAR_STATE = 0x03;

/**
 * Registry of audio-native adapter factories
 */
const ADAPTER_FACTORIES: Record<string, (apiKey?: string) => IAudioNativeAdapter> = {
  'qwen3-omni-flash-realtime': (apiKey) => new Qwen3OmniRealtimeAdapter(apiKey),
  'qwen3-omni': (apiKey) => new Qwen3OmniRealtimeAdapter(apiKey),
  'gemini-2.5-flash-native-audio-preview': (apiKey) => new GeminiLiveAdapter(apiKey),
  'gemini-live': (apiKey) => new GeminiLiveAdapter(apiKey),
  'gpt-4o-realtime': (apiKey) => new GPT4oRealtimeAdapter(apiKey),
  'gpt-4o-realtime-preview': (apiKey) => new GPT4oRealtimeAdapter(apiKey),
  'gpt-4o-realtime-preview-2024-12-17': (apiKey) => new GPT4oRealtimeAdapter(apiKey),
};

// CallMessage types matching Rust call_server.rs
interface JoinMessage {
  type: 'Join';
  call_id: string;
  user_id: string;
  display_name: string;
  is_ai: boolean;
}

interface LeaveMessage {
  type: 'Leave';
}

/**
 * Active connection: adapter + WebSocket stream to call server
 */
interface ActiveConnection extends AudioNativeConnection {
  adapter: IAudioNativeAdapter;
  /** WebSocket to call server for receiving mixed room audio as a stream */
  ws: WebSocket | null;
  /** Adapter's output sample rate (e.g. 24000 for Qwen3-Omni) */
  outputSampleRate: number;
  /** Adapter's input sample rate (e.g. 16000) */
  inputSampleRate: number;
  /** Accumulated output samples for current response (for duration calculation) */
  responseSampleCount: number;
  /** Frame counters for periodic logging (don't log every 20ms frame) */
  roomAudioFrameCount: number;
  injectedAudioChunkCount: number;
}

/**
 * AudioNativeBridge - Singleton managing all audio-native AI connections
 *
 * Each audio-native AI gets:
 * 1. A WebSocket to the call server (receives mixed room audio as binary stream)
 * 2. An adapter connection to the model's realtime API
 * 3. An IPC client for injecting audio back into the mixer
 */
export class AudioNativeBridge {
  private static _instance: AudioNativeBridge | null = null;
  private connections: Map<string, ActiveConnection> = new Map();
  private ipcClient: RustCoreIPCClient;

  private constructor() {
    this.ipcClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
    this.ipcClient.connect().catch(() => {});
  }

  static get instance(): AudioNativeBridge {
    if (!AudioNativeBridge._instance) {
      AudioNativeBridge._instance = new AudioNativeBridge();
    }
    return AudioNativeBridge._instance;
  }

  /**
   * Check if a model is audio-native
   */
  isAudioNativeModel(modelId: string): boolean {
    if (ADAPTER_FACTORIES[modelId]) {
      return true;
    }

    const audioNativeModels = [
      'qwen3-omni',
      'gpt-4o-realtime',
      'gpt-realtime',
      'gemini-2.0-flash',
      'gemini-2.5-flash',
      'gemini-live',
      'nova-sonic',
    ];

    return audioNativeModels.some(prefix => modelId.toLowerCase().includes(prefix));
  }

  /**
   * Connect an audio-native AI to a voice call.
   *
   * Sets up bidirectional audio streaming:
   * - Room audio → WebSocket → adapter.sendAudio() (AI hears the room)
   * - adapter.onAudioOutput() → IPC inject → mixer (AI speaks to the room)
   */
  async joinCall(
    callId: string,
    userId: UUID,
    displayName: string,
    modelId: string,
    config?: Partial<AudioNativeSessionConfig>
  ): Promise<boolean> {
    const key = `${callId}-${userId}`;

    if (this.connections.has(key)) {
      return true;
    }

    const factory = ADAPTER_FACTORIES[modelId] ||
                    ADAPTER_FACTORIES[this.normalizeModelId(modelId)];

    if (!factory) {
      return false;
    }

    try {
      const adapter = factory();
      const voice = this.selectVoice(userId, modelId);

      const sessionConfig: AudioNativeSessionConfig = {
        ...DEFAULT_AUDIO_NATIVE_CONFIG,
        ...config,
        voice,
        instructions: `You are ${displayName}, participating in a voice conversation. Be natural, conversational, and concise.`,
      };

      // Connect to the model's realtime API
      await adapter.connect(sessionConfig);

      // Store connection
      const connection: ActiveConnection = {
        userId,
        displayName,
        modelId,
        callId,
        isConnected: true,
        adapter,
        ws: null,
        outputSampleRate: sessionConfig.outputAudioFormat.sampleRate,
        inputSampleRate: sessionConfig.inputAudioFormat.sampleRate,
        responseSampleCount: 0,
        roomAudioFrameCount: 0,
        injectedAudioChunkCount: 0,
      };
      this.connections.set(key, connection);

      // Set up adapter event handlers (output audio → mixer injection)
      this.setupAdapterHandlers(adapter, callId, userId, displayName, connection);

      // Connect WebSocket to call server for streaming room audio
      await this.connectCallServerStream(key, connection);

      return true;

    } catch {
      return false;
    }
  }

  /**
   * Connect WebSocket to call server for receiving mixed room audio.
   * Binary frames from the server contain mix-minus audio (everything except this AI's own audio).
   * These frames are forwarded to the adapter so the AI can hear the room in real-time.
   */
  private connectCallServerStream(key: string, connection: ActiveConnection): Promise<void> {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(STREAMING_CORE_URL);
      connection.ws = ws;

      ws.on('open', () => {
        // Join as AI participant — server creates ring buffer for audio buffering
        const joinMsg: JoinMessage = {
          type: 'Join',
          call_id: connection.callId,
          user_id: connection.userId,
          display_name: connection.displayName,
          is_ai: true,
        };
        ws.send(JSON.stringify(joinMsg));
        resolve();
      });

      // Binary frames = mixed room audio (i16 LE PCM at 16kHz)
      // Forward to adapter so the AI can hear the room
      ws.on('message', (data: WebSocket.Data) => {
        if (Buffer.isBuffer(data) || data instanceof ArrayBuffer) {
          this.handleRoomAudioFrame(key, data);
        }
        // JSON messages (transcriptions, etc.) are ignored — audio-native models
        // hear the room directly, they don't need text transcriptions
      });

      ws.on('error', (error) => {
        reject(error);
      });

      ws.on('close', () => {
        connection.ws = null;
      });
    });
  }

  /**
   * Handle a binary frame from the call server.
   * Binary frame protocol: first byte is FrameKind discriminator.
   *   0x01 = Audio (PCM16 i16 LE), 0x02 = Video, 0x03 = AvatarState
   * Forwards audio to the audio-native model adapter.
   */
  private handleRoomAudioFrame(key: string, data: WebSocket.Data): void {
    const connection = this.connections.get(key);
    if (!connection || !connection.adapter.isConnected()) return;

    const buffer = Buffer.isBuffer(data) ? data : Buffer.from(data as ArrayBuffer);
    if (buffer.length < 2) return;

    // Check FrameKind discriminator (first byte)
    const frameKind = buffer[0];
    let pcmBuffer: Buffer;

    if (frameKind === FRAME_KIND_AUDIO) {
      // New protocol: [0x01][PCM16 data...]
      pcmBuffer = buffer.subarray(1);
    } else if (frameKind === FRAME_KIND_VIDEO || frameKind === FRAME_KIND_AVATAR_STATE) {
      // Video and avatar state frames — not handled by audio-native adapters yet
      return;
    } else {
      // Legacy: no FrameKind prefix, treat entire buffer as raw audio
      pcmBuffer = buffer;
    }

    // Convert bytes to Int16Array (little-endian PCM from call server)
    const samples = new Int16Array(pcmBuffer.buffer, pcmBuffer.byteOffset, pcmBuffer.byteLength / 2);

    connection.roomAudioFrameCount++;

    // Resample if adapter expects different rate than call server (16kHz)
    if (connection.inputSampleRate !== CALL_SERVER_SAMPLE_RATE) {
      const resampled = linearResample(samples, CALL_SERVER_SAMPLE_RATE, connection.inputSampleRate);
      connection.adapter.sendAudio(resampled);
    } else {
      connection.adapter.sendAudio(samples);
    }
  }

  /**
   * Disconnect AI from a voice call
   */
  async leaveCall(callId: string, userId: UUID): Promise<void> {
    const key = `${callId}-${userId}`;
    const connection = this.connections.get(key);

    if (connection) {
      // Disconnect adapter from model's realtime API
      await connection.adapter.disconnect();

      // Disconnect WebSocket from call server
      if (connection.ws && connection.ws.readyState === WebSocket.OPEN) {
        const leaveMsg: LeaveMessage = { type: 'Leave' };
        connection.ws.send(JSON.stringify(leaveMsg));
        connection.ws.close();
      }

      this.connections.delete(key);
    }
  }

  /**
   * Cancel response for an AI (for interruption handling)
   */
  cancelResponse(callId: string, userId: UUID): void {
    const key = `${callId}-${userId}`;
    const connection = this.connections.get(key);
    if (connection) {
      connection.adapter.cancelResponse();
    }
  }

  /**
   * Check if AI is in a call
   */
  isInCall(callId: string, userId: UUID): boolean {
    const key = `${callId}-${userId}`;
    const connection = this.connections.get(key);
    return connection?.isConnected ?? false;
  }

  /**
   * Get all audio-native participants in a call
   */
  getParticipants(callId: string): AudioNativeConnection[] {
    const participants: AudioNativeConnection[] = [];
    for (const [key, connection] of this.connections) {
      if (key.startsWith(callId)) {
        participants.push({
          userId: connection.userId,
          displayName: connection.displayName,
          modelId: connection.modelId,
          callId: connection.callId,
          isConnected: connection.isConnected,
        });
      }
    }
    return participants;
  }

  // ============================================================================
  // Private Methods
  // ============================================================================

  /**
   * Set up event handlers for adapter output.
   * When the audio-native model produces audio, inject it into the call mixer.
   */
  private setupAdapterHandlers(
    adapter: IAudioNativeAdapter,
    callId: string,
    userId: UUID,
    displayName: string,
    connection: ActiveConnection
  ): void {
    // Audio output from the AI → inject into call mixer via IPC
    // Also track accumulated sample count for duration calculation
    adapter.onAudioOutput((samples) => {
      connection.responseSampleCount += samples.length;
      this.injectAudioToMixer(callId, userId, displayName, samples, connection.outputSampleRate);
    });

    // Transcripts → broadcast so text-based AIs can see what was said
    // On final transcript, calculate audio duration from accumulated samples
    adapter.onTranscript((text, isFinal) => {
      if (isFinal) {
        const audioDurationMs = connection.outputSampleRate > 0
          ? Math.round((connection.responseSampleCount / connection.outputSampleRate) * 1000)
          : 0;
        this.handleTranscript(callId, userId, displayName, text, audioDurationMs);
        // Reset for next response
        connection.responseSampleCount = 0;
      }
    });

    // Speech detection → turn-taking coordination
    adapter.onSpeechDetected((started) => {
      if (started) {
        // New speech detected from input — reset sample counter for fresh response
        connection.responseSampleCount = 0;
      }
      Events.emit('voice:audio-native:speech-detected', {
        callId,
        userId,
        displayName,
        started,
        timestamp: Date.now(),
      });
    });

    adapter.onError(() => {
      // Adapter error — handled by caller
    });
  }

  /**
   * Inject audio-native model output into the call mixer.
   * Resamples from adapter output rate (e.g. 24kHz) to call server rate (16kHz)
   * and sends via IPC to Rust call_manager.inject_audio().
   */
  private async injectAudioToMixer(
    callId: string,
    userId: UUID,
    displayName: string,
    samples: Int16Array,
    adapterSampleRate: number
  ): Promise<void> {
    try {
      // Ensure IPC is connected
      if (!this.ipcClient.connected) {
        await this.ipcClient.connect();
      }

      // Resample to call server rate if needed
      const resampled = adapterSampleRate === CALL_SERVER_SAMPLE_RATE
        ? samples
        : linearResample(samples, adapterSampleRate, CALL_SERVER_SAMPLE_RATE);

      // Inject via IPC — audio goes directly into the Rust mixer
      await this.ipcClient.voiceInjectAudio(callId, userId, Array.from(resampled));

      const key = `${callId}-${userId}`;
      const conn = this.connections.get(key);
      if (conn) {
        conn.injectedAudioChunkCount++;
      }
    } catch {
      // Injection failed — audio frame dropped
    }
  }

  /**
   * Handle transcript from an audio-native AI (broadcast so text-based AIs see it)
   */
  private async handleTranscript(
    callId: string,
    userId: UUID,
    displayName: string,
    text: string,
    audioDurationMs: number
  ): Promise<void> {
    if (DataDaemon.jtagContext) {
      await Events.emit(
        DataDaemon.jtagContext,
        'voice:ai:speech',
        {
          sessionId: callId,
          speakerId: userId,
          speakerName: displayName,
          text,
          audioDurationMs,
          isAudioNative: true,
          timestamp: Date.now(),
        },
        { scope: EVENT_SCOPES.GLOBAL }
      );
    }
  }

  /**
   * Normalize model ID to match factory keys
   */
  private normalizeModelId(modelId: string): string {
    const lower = modelId.toLowerCase();

    if (lower.includes('qwen3-omni') || lower.includes('qwen-omni')) {
      return 'qwen3-omni-flash-realtime';
    }

    if (lower.includes('gemini') && (lower.includes('native-audio') || lower.includes('live'))) {
      return 'gemini-2.5-flash-native-audio-preview';
    }

    if (lower.includes('gpt-4o-realtime') || lower.includes('gpt-realtime')) {
      return 'gpt-4o-realtime';
    }

    return modelId;
  }

  /**
   * Select a voice deterministically from userId
   */
  private selectVoice(userId: string, modelId: string): string {
    let voices: readonly string[];
    const lower = modelId.toLowerCase();

    if (lower.includes('gemini')) {
      voices = AUDIO_NATIVE_VOICES['gemini-live'] ?? ['Aoede'];
    } else if (lower.includes('gpt') || lower.includes('openai')) {
      voices = AUDIO_NATIVE_VOICES['gpt-realtime'] ?? ['alloy'];
    } else {
      voices = AUDIO_NATIVE_VOICES['qwen3-omni'] ?? ['Cherry'];
    }

    let hash = 0;
    for (let i = 0; i < userId.length; i++) {
      hash = (hash * 31 + userId.charCodeAt(i)) >>> 0;
    }

    return voices[hash % voices.length];
  }
}

// ============================================================================
// Audio Utilities
// ============================================================================

/**
 * Linear interpolation resampling between sample rates.
 * Used for converting between call server (16kHz) and audio-native models (24kHz).
 *
 * For production, this is adequate for speech audio — it preserves intelligibility.
 * If we need higher quality later, we can switch to sinc interpolation.
 */
function linearResample(input: Int16Array, fromRate: number, toRate: number): Int16Array {
  if (fromRate === toRate) return input;

  const ratio = fromRate / toRate;
  const outputLen = Math.ceil(input.length / ratio);
  const output = new Int16Array(outputLen);

  for (let i = 0; i < outputLen; i++) {
    const srcIdx = i * ratio;
    const srcFloor = Math.floor(srcIdx);
    const srcCeil = Math.min(srcFloor + 1, input.length - 1);
    const frac = srcIdx - srcFloor;

    // Linear interpolation between adjacent samples
    output[i] = Math.round(input[srcFloor] * (1 - frac) + input[srcCeil] * frac);
  }

  return output;
}

/**
 * Singleton accessor
 */
export function getAudioNativeBridge(): AudioNativeBridge {
  return AudioNativeBridge.instance;
}
