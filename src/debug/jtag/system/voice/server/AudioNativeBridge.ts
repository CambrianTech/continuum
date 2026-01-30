/**
 * AudioNativeBridge - Manages audio-native AI connections to voice calls
 *
 * Unlike AIAudioBridge (which handles text-based models via TTS/STT),
 * this bridge connects audio-native models that can:
 * - Hear raw audio directly
 * - Speak audio directly (no TTS needed)
 *
 * Supported models:
 * - Qwen3-Omni (Alibaba) - Open source, self-hostable
 * - GPT-4o Realtime (OpenAI) - Closed source
 * - Gemini Live (Google) - Closed source
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type {
  IAudioNativeAdapter,
  AudioNativeConnection,
  AudioNativeSessionConfig,
} from '../shared/AudioNativeTypes';
import { DEFAULT_AUDIO_NATIVE_CONFIG, AUDIO_NATIVE_VOICES } from '../shared/AudioNativeTypes';
import { Qwen3OmniRealtimeAdapter } from './adapters/Qwen3OmniRealtimeAdapter';
import { GeminiLiveAdapter } from './adapters/GeminiLiveAdapter';
import { Events } from '../../core/shared/Events';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import { EVENT_SCOPES } from '../../events/shared/EventSystemConstants';

/**
 * Registry of audio-native adapter factories
 */
const ADAPTER_FACTORIES: Record<string, (apiKey?: string) => IAudioNativeAdapter> = {
  // Qwen3-Omni (Alibaba DashScope)
  'qwen3-omni-flash-realtime': (apiKey) => new Qwen3OmniRealtimeAdapter(apiKey),
  'qwen3-omni': (apiKey) => new Qwen3OmniRealtimeAdapter(apiKey),
  // Gemini Live (Google) - Free tier available
  'gemini-2.5-flash-native-audio-preview': (apiKey) => new GeminiLiveAdapter(apiKey),
  'gemini-live': (apiKey) => new GeminiLiveAdapter(apiKey),
  // Future: Add OpenAI gpt-realtime
};

/**
 * Active connection with adapter
 */
interface ActiveConnection extends AudioNativeConnection {
  adapter: IAudioNativeAdapter;
}

/**
 * AudioNativeBridge - Singleton managing all audio-native AI connections
 */
export class AudioNativeBridge {
  private static _instance: AudioNativeBridge | null = null;
  private connections: Map<string, ActiveConnection> = new Map(); // keyed by `${callId}-${userId}`

  private constructor() {
    console.log('🎙️ AudioNativeBridge: Initialized');
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
    // Check if we have a factory for this model
    if (ADAPTER_FACTORIES[modelId]) {
      return true;
    }

    // Check prefixes for versioned models
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
   * Connect an audio-native AI to a voice call
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
      console.log(`🎙️ AudioNativeBridge: ${displayName} already in call`);
      return true;
    }

    // Find adapter factory
    const factory = ADAPTER_FACTORIES[modelId] ||
                    ADAPTER_FACTORIES[this.normalizeModelId(modelId)];

    if (!factory) {
      console.error(`🎙️ AudioNativeBridge: No adapter for model ${modelId}`);
      return false;
    }

    try {
      const adapter = factory();

      // Select voice deterministically from userId
      const voice = this.selectVoice(userId, modelId);

      // Connect to the model's realtime API
      await adapter.connect({
        ...DEFAULT_AUDIO_NATIVE_CONFIG,
        ...config,
        voice,
        instructions: `You are ${displayName}, participating in a voice conversation. Be natural, conversational, and concise.`,
      });

      // Set up event handlers
      this.setupAdapterHandlers(adapter, callId, userId, displayName);

      // Store connection
      const connection: ActiveConnection = {
        userId,
        displayName,
        modelId,
        callId,
        isConnected: true,
        adapter,
      };
      this.connections.set(key, connection);

      console.log(`🎙️ AudioNativeBridge: ${displayName} (${modelId}) joined call ${callId.slice(0, 8)}`);
      return true;

    } catch (error) {
      console.error(`🎙️ AudioNativeBridge: Failed to connect ${displayName}:`, error);
      return false;
    }
  }

  /**
   * Disconnect AI from a voice call
   */
  async leaveCall(callId: string, userId: UUID): Promise<void> {
    const key = `${callId}-${userId}`;
    const connection = this.connections.get(key);

    if (connection) {
      await connection.adapter.disconnect();
      this.connections.delete(key);
      console.log(`🎙️ AudioNativeBridge: ${connection.displayName} left call`);
    }
  }

  /**
   * Send audio to an audio-native AI
   *
   * Call this with raw audio samples from the call mixer.
   * The AI will hear this audio and potentially respond.
   */
  sendAudio(callId: string, userId: UUID, samples: Int16Array): void {
    const key = `${callId}-${userId}`;
    const connection = this.connections.get(key);

    if (connection && connection.adapter.isConnected()) {
      connection.adapter.sendAudio(samples);
    }
  }

  /**
   * Send audio to ALL audio-native AIs in a call
   *
   * This is used to broadcast mixed audio to all audio-native participants.
   */
  broadcastAudio(callId: string, samples: Int16Array, excludeUserId?: UUID): void {
    for (const [key, connection] of this.connections) {
      if (key.startsWith(callId) && connection.userId !== excludeUserId) {
        if (connection.adapter.isConnected()) {
          connection.adapter.sendAudio(samples);
        }
      }
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
   * Set up event handlers for an adapter
   */
  private setupAdapterHandlers(
    adapter: IAudioNativeAdapter,
    callId: string,
    userId: UUID,
    displayName: string
  ): void {
    // Handle audio output from the AI
    adapter.onAudioOutput((samples) => {
      this.handleAudioOutput(callId, userId, displayName, samples);
    });

    // Handle transcripts (what the AI is saying)
    adapter.onTranscript((text, isFinal) => {
      if (isFinal) {
        this.handleTranscript(callId, userId, displayName, text);
      }
    });

    // Handle speech detection (for turn-taking)
    adapter.onSpeechDetected((started) => {
      this.handleSpeechDetected(callId, userId, displayName, started);
    });

    // Handle errors
    adapter.onError((error) => {
      console.error(`🎙️ AudioNativeBridge: Error from ${displayName}:`, error);
    });
  }

  /**
   * Handle audio output from an audio-native AI
   *
   * This audio needs to be injected into the call mixer so
   * humans and other participants can hear it.
   */
  private handleAudioOutput(
    callId: string,
    userId: UUID,
    displayName: string,
    samples: Int16Array
  ): void {
    // Emit event for VoiceWebSocketHandler to inject into call
    // The audio is 24kHz from Qwen3-Omni, may need resampling to 16kHz
    Events.emit('voice:audio-native:output', {
      callId,
      userId,
      displayName,
      samples: Array.from(samples), // Convert to regular array for event serialization
      sampleRate: 24000,
    });
  }

  /**
   * Handle transcript from an audio-native AI (what they said)
   */
  private async handleTranscript(
    callId: string,
    userId: UUID,
    displayName: string,
    text: string
  ): Promise<void> {
    console.log(`🎙️ AudioNativeBridge: ${displayName} said: "${text.slice(0, 50)}..."`);

    // Broadcast to other participants (for text-based AIs to see)
    if (DataDaemon.jtagContext) {
      await Events.emit(
        DataDaemon.jtagContext,
        'voice:ai:speech',
        {
          sessionId: callId,
          speakerId: userId,
          speakerName: displayName,
          text,
          isAudioNative: true,
          timestamp: Date.now(),
        },
        { scope: EVENT_SCOPES.GLOBAL }
      );
    }
  }

  /**
   * Handle speech detection from audio-native AI's VAD
   */
  private handleSpeechDetected(
    callId: string,
    userId: UUID,
    displayName: string,
    started: boolean
  ): void {
    // Emit for turn-taking coordination
    Events.emit('voice:audio-native:speech-detected', {
      callId,
      userId,
      displayName,
      started,
      timestamp: Date.now(),
    });
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

    return modelId;
  }

  /**
   * Select a voice deterministically from userId
   */
  private selectVoice(userId: string, modelId: string): string {
    // Determine voice set based on model
    let voices: readonly string[];
    const lower = modelId.toLowerCase();

    if (lower.includes('gemini')) {
      voices = AUDIO_NATIVE_VOICES['gemini-live'] ?? ['Aoede'];
    } else if (lower.includes('gpt') || lower.includes('openai')) {
      voices = AUDIO_NATIVE_VOICES['gpt-realtime'] ?? ['alloy'];
    } else {
      voices = AUDIO_NATIVE_VOICES['qwen3-omni'] ?? ['Cherry'];
    }

    // Simple hash to select voice
    let hash = 0;
    for (let i = 0; i < userId.length; i++) {
      hash = (hash * 31 + userId.charCodeAt(i)) >>> 0;
    }

    const voiceIndex = hash % voices.length;
    return voices[voiceIndex];
  }
}

/**
 * Singleton accessor
 */
export function getAudioNativeBridge(): AudioNativeBridge {
  return AudioNativeBridge.instance;
}
