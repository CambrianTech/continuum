/**
 * VoiceOrchestrator - Bridges voice transcriptions with persona system
 *
 * Responsibilities:
 * 1. Receive transcription events from VoiceWebSocketHandler
 * 2. Broadcast transcripts to ALL text-based AIs (audio-native AIs hear via mixer)
 * 3. Route persona responses to TTS
 *
 * NO turn-taking gating. NO cooldowns. NO arbiter selection.
 * Every text-based AI gets every utterance. They each decide independently
 * whether to respond. This is a GROUP CONVERSATION, not a turn-based game.
 *
 * Audio-native AIs (Gemini Live, Qwen3-Omni, GPT-4o Realtime) hear raw audio
 * through the mixer's mix-minus stream and are excluded from text broadcasts.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { InboxMessage } from '../../user/server/modules/QueueItemTypes';
import { Events } from '../../core/shared/Events';
import type { UserEntity } from '../../data/entities/UserEntity';
import { getAIAudioBridge } from './AIAudioBridge';
import { getAudioNativeBridge } from './AudioNativeBridge';
import { registerVoiceOrchestrator } from '../../rag/sources/VoiceConversationSource';
import { DataList } from '../../../commands/data/list/shared/DataListTypes';
/**
 * Utterance event from voice transcription
 */
export interface UtteranceEvent {
  sessionId: UUID;           // Voice call session ID
  speakerId: UUID;           // Who spoke
  speakerName: string;       // Display name
  speakerType: 'human' | 'persona' | 'agent';
  transcript: string;        // Transcribed text
  confidence: number;        // Transcription confidence (0-1)
  audioRef?: string;         // Reference to audio chunk (for replay)
  timestamp: number;         // When utterance completed
}

/**
 * Voice participant info
 */
interface VoiceParticipant {
  userId: UUID;
  displayName: string;
  type: 'human' | 'persona' | 'agent';
  personaUser?: unknown;     // PersonaUser instance if AI
  expertise?: string[];      // For relevance scoring
  modelId?: string;          // AI model ID (e.g., 'qwen3-omni', 'claude-3-sonnet')
  isAudioNative?: boolean;   // True if model supports direct audio I/O
}

/**
 * Session context - tracks recent conversation for RAG
 */
interface SessionContext {
  sessionId: UUID;
  roomId: UUID;
  recentUtterances: UtteranceEvent[];
  turnCount: number;
}

/**
 * VoiceOrchestrator - Central coordinator for voice ↔ persona integration
 */
export class VoiceOrchestrator {
  private static _instance: VoiceOrchestrator | null = null;

  // Session state
  private sessionParticipants: Map<UUID, VoiceParticipant[]> = new Map();
  private sessionContexts: Map<UUID, SessionContext> = new Map();

  private constructor() {
    this.setupEventListeners();

    // Register with VoiceConversationSource for RAG context building
    registerVoiceOrchestrator(this);

    console.log('🎙️ VoiceOrchestrator: Initialized');
  }

  static get instance(): VoiceOrchestrator {
    if (!VoiceOrchestrator._instance) {
      VoiceOrchestrator._instance = new VoiceOrchestrator();
    }
    return VoiceOrchestrator._instance;
  }

  /**
   * Register participants for a voice session
   */
  async registerSession(sessionId: UUID, roomId: UUID, participantIds: UUID[]): Promise<void> {
    const participants: VoiceParticipant[] = [];

    // Look up users from database
    if (participantIds.length > 0) {
      try {
        const result = await DataList.execute<UserEntity>({
            collection: 'users',
            filter: { id: { $in: participantIds } },
            limit: participantIds.length
          }
        );

        if (result.success && result.items) {
          const audioNativeBridge = getAudioNativeBridge();
          for (const user of result.items) {
            const metadata = user.metadata as Record<string, unknown> | undefined;
            const modelId = metadata?.modelId as string | undefined;
            const isAudioNative = modelId ? audioNativeBridge.isAudioNativeModel(modelId) : false;

            participants.push({
              userId: user.id as UUID,
              displayName: user.displayName || user.uniqueId,
              type: user.type as 'human' | 'persona' | 'agent',
              expertise: metadata?.expertise as string[] | undefined,
              modelId,
              isAudioNative,
            });
          }
        }
      } catch (error) {
        console.warn('🎙️ VoiceOrchestrator: Failed to load participants:', error);
      }
    }

    this.sessionParticipants.set(sessionId, participants);
    this.sessionContexts.set(sessionId, {
      sessionId,
      roomId,
      recentUtterances: [],
      turnCount: 0
    });

    console.log(`🎙️ VoiceOrchestrator: Registered session ${sessionId.slice(0, 8)} for room ${roomId.slice(0, 8)} with ${participants.length} participants`);

    // Connect AI participants to the appropriate audio bridge
    const aiParticipants = participants.filter(p => p.type === 'persona' || p.type === 'agent');
    if (aiParticipants.length > 0) {
      const textBridge = getAIAudioBridge();
      const audioNativeBridge = getAudioNativeBridge();

      for (const ai of aiParticipants) {
        if (ai.isAudioNative && ai.modelId) {
          // Audio-native models: connect via AudioNativeBridge (direct audio I/O)
          console.log(`🎙️ VoiceOrchestrator: Connecting ${ai.displayName} (${ai.modelId}) as AUDIO-NATIVE...`);
          audioNativeBridge.joinCall(sessionId, ai.userId, ai.displayName, ai.modelId).then(success => {
            if (success) {
              console.log(`🎙️ VoiceOrchestrator: ${ai.displayName} connected as audio-native`);
            } else {
              console.warn(`🎙️ VoiceOrchestrator: ${ai.displayName} failed to connect (falling back to text)`);
              // Fallback to text-based bridge
              textBridge.joinCall(sessionId, ai.userId, ai.displayName);
            }
          });
        } else {
          // Text-based models: connect via AIAudioBridge (STT → LLM → TTS)
          console.log(`🎙️ VoiceOrchestrator: Connecting ${ai.displayName} as TEXT-BASED...`);
          textBridge.joinCall(sessionId, ai.userId, ai.displayName).then(success => {
            if (success) {
              console.log(`🎙️ VoiceOrchestrator: ${ai.displayName} connected to audio`);
            } else {
              console.warn(`🎙️ VoiceOrchestrator: ${ai.displayName} failed to connect to audio`);
            }
          });
        }
      }
    }
  }

  /**
   * Unregister a voice session
   */
  unregisterSession(sessionId: UUID): void {
    // Disconnect AI participants from both bridges
    const participants = this.sessionParticipants.get(sessionId);
    if (participants) {
      const textBridge = getAIAudioBridge();
      const audioNativeBridge = getAudioNativeBridge();
      const aiParticipants = participants.filter(p => p.type === 'persona' || p.type === 'agent');

      for (const ai of aiParticipants) {
        if (ai.isAudioNative) {
          audioNativeBridge.leaveCall(sessionId, ai.userId);
        } else {
          textBridge.leaveCall(sessionId, ai.userId);
        }
      }
    }

    this.sessionParticipants.delete(sessionId);
    this.sessionContexts.delete(sessionId);
    console.log(`🎙️ VoiceOrchestrator: Unregistered session ${sessionId.slice(0, 8)}`);
  }

  /**
   * Handle incoming utterance from transcription
   *
   * Broadcasts to ALL text-based AI participants. No gating. No cooldowns.
   * No arbiter selection. Each AI decides independently whether to respond.
   */
  async onUtterance(event: UtteranceEvent): Promise<void> {
    const { sessionId, speakerId, transcript, speakerName } = event;

    console.log(`🎙️ VoiceOrchestrator: Utterance from ${speakerName}: "${transcript.slice(0, 50)}..."`);

    const context = this.sessionContexts.get(sessionId);
    if (!context) {
      console.error(`🎙️ VoiceOrchestrator: No context for session ${sessionId.slice(0, 8)} — was registerSession() called?`);
      return;
    }

    const participants = this.sessionParticipants.get(sessionId);
    if (!participants || participants.length === 0) {
      console.error(`🎙️ VoiceOrchestrator: No participants for session ${sessionId.slice(0, 8)}`);
      return;
    }

    // Update context
    context.recentUtterances.push(event);
    if (context.recentUtterances.length > 20) {
      context.recentUtterances.shift();
    }
    context.turnCount++;

    // Get TEXT-BASED AI participants (excluding speaker AND audio-native AIs)
    // Audio-native AIs hear raw audio through mixer — text would cause double response
    const textAIs = participants.filter(
      p => p.type === 'persona' && p.userId !== speakerId && !p.isAudioNative
    );

    if (textAIs.length === 0) {
      console.log('🎙️ VoiceOrchestrator: No text-based AIs to notify');
      return;
    }

    // Broadcast to ALL text-based AIs — each gets the utterance
    console.log(`🎙️ VoiceOrchestrator: Broadcasting to ${textAIs.length} text-based AIs`);
    for (const ai of textAIs) {
      Events.emit('voice:transcription:directed', {
        sessionId: event.sessionId,
        speakerId: event.speakerId,
        speakerName: event.speakerName,
        speakerType: event.speakerType,
        transcript: event.transcript,
        confidence: event.confidence,
        language: 'en',
        timestamp: event.timestamp,
        targetPersonaId: ai.userId
      });
    }
  }

  /**
   * Handle persona response — route to TTS.
   * No fallbacks. If the AI isn't in the call, that's an error.
   */
  async onPersonaResponse(
    personaId: UUID,
    response: string,
    originalMessage: InboxMessage
  ): Promise<void> {
    if (originalMessage.sourceModality !== 'voice' || !originalMessage.voiceSessionId) {
      return;
    }

    const sessionId = originalMessage.voiceSessionId;

    const bridge = getAIAudioBridge();
    if (!bridge.isInCall(sessionId, personaId)) {
      console.error(`🎙️ VoiceOrchestrator: AI ${personaId.slice(0, 8)} NOT in call ${sessionId.slice(0, 8)} — cannot speak. WebSocket connection failed or was never established.`);
      return;
    }

    await bridge.speak(sessionId, personaId, response);
  }

  /**
   * Check if a message should be routed to voice
   */
  isVoiceMessage(message: InboxMessage): boolean {
    return message.sourceModality === 'voice' && !!message.voiceSessionId;
  }

  /**
   * Setup event listeners for persona responses and incoming transcriptions
   */
  private setupEventListeners(): void {
    // Listen for persona responses that might need TTS routing
    Events.subscribe('persona:response:generated', async (event: {
      personaId: UUID;
      response: string;
      originalMessage: InboxMessage;
    }) => {
      if (this.isVoiceMessage(event.originalMessage)) {
        await this.onPersonaResponse(event.personaId, event.response, event.originalMessage);
      }
    });

    // Listen for transcriptions from Rust streaming-core via browser
    // This bridges the Rust call_server's Whisper STT to the persona system
    Events.subscribe('voice:transcription', async (event: {
      sessionId: string;
      speakerId: string;
      speakerName: string;
      transcript: string;
      confidence: number;
      language: string;
      timestamp: number;
    }) => {
      console.log(`[STEP 10] 🎙️ VoiceOrchestrator RECEIVED event: "${event.transcript.slice(0, 50)}..."`);

      // Convert to UtteranceEvent and process
      const utteranceEvent: UtteranceEvent = {
        sessionId: event.sessionId as UUID,
        speakerId: event.speakerId as UUID,
        speakerName: event.speakerName,
        speakerType: 'human',  // Could be enhanced to detect AI speakers
        transcript: event.transcript,
        confidence: event.confidence,
        timestamp: event.timestamp
      };

      console.log(`[STEP 11] 🎯 VoiceOrchestrator broadcasting to all text-based AIs`);
      await this.onUtterance(utteranceEvent);
    });

    // Listen for mid-call participant joins
    // When a new AI joins an active call, connect them to the appropriate audio bridge
    Events.subscribe('live:participant:joined', async (event: {
      sessionId: string;
      userId: string;
      displayName: string;
      type: 'human' | 'persona' | 'agent';
    }) => {
      const sessionId = event.sessionId as UUID;
      const participants = this.sessionParticipants.get(sessionId);
      if (!participants) return; // Not a tracked voice session

      // Check if this participant is already registered
      if (participants.some(p => p.userId === event.userId)) return;

      // Look up user to get modelId and determine audio-native status
      if (event.type === 'persona' || event.type === 'agent') {
        try {
          const result = await DataList.execute<UserEntity>({
            collection: 'users',
            filter: { id: event.userId },
            limit: 1,
          });

          if (result.success && result.items?.length) {
            const user = result.items[0];
            const metadata = user.metadata as Record<string, unknown> | undefined;
            const modelId = metadata?.modelId as string | undefined;
            const audioNativeBridge = getAudioNativeBridge();
            const isAudioNative = modelId ? audioNativeBridge.isAudioNativeModel(modelId) : false;

            const participant: VoiceParticipant = {
              userId: event.userId as UUID,
              displayName: event.displayName,
              type: event.type,
              expertise: metadata?.expertise as string[] | undefined,
              modelId,
              isAudioNative,
            };

            participants.push(participant);

            // Connect to appropriate bridge
            if (isAudioNative && modelId) {
              console.log(`🎙️ VoiceOrchestrator: Mid-call join: ${event.displayName} (${modelId}) as AUDIO-NATIVE`);
              const success = await audioNativeBridge.joinCall(sessionId, participant.userId, participant.displayName, modelId);
              if (!success) {
                const textBridge = getAIAudioBridge();
                textBridge.joinCall(sessionId, participant.userId, participant.displayName);
              }
            } else {
              console.log(`🎙️ VoiceOrchestrator: Mid-call join: ${event.displayName} as TEXT-BASED`);
              const textBridge = getAIAudioBridge();
              textBridge.joinCall(sessionId, participant.userId, participant.displayName);
            }
          }
        } catch (error) {
          console.warn(`🎙️ VoiceOrchestrator: Failed to add mid-call joiner ${event.displayName}:`, error);
        }
      }
    });

    // Listen for AI speech events — just log for visibility, no gating
    Events.subscribe('voice:ai:speech', (event: {
      sessionId: string;
      speakerId: string;
      speakerName: string;
      text: string;
      audioDurationMs?: number;
      failed?: boolean;
      timestamp: number;
    }) => {
      if (event.failed) {
        console.error(`🎙️ VoiceOrchestrator: AI ${event.speakerName} speech FAILED`);
      } else {
        console.log(`🎙️ VoiceOrchestrator: AI ${event.speakerName} spoke ${event.audioDurationMs ?? 0}ms`);
      }
    });
  }

  /**
   * Get session statistics
   */
  getSessionStats(sessionId: UUID): { participants: number; turnCount: number } | null {
    const participants = this.sessionParticipants.get(sessionId);
    const context = this.sessionContexts.get(sessionId);

    if (!participants || !context) return null;

    return {
      participants: participants.length,
      turnCount: context.turnCount,
    };
  }

  /**
   * Get recent utterances for a voice session
   * Used by VoiceConversationSource for RAG context building
   *
   * @param sessionId - Voice session ID
   * @param limit - Maximum number of utterances to return (default: 20)
   * @returns Array of recent utterances with speaker type information
   */
  getRecentUtterances(sessionId: string, limit: number = 20): UtteranceEvent[] {
    const context = this.sessionContexts.get(sessionId as UUID);
    if (!context) {
      return [];
    }

    // Return most recent utterances up to limit
    const utterances = context.recentUtterances.slice(-limit);
    return utterances;
  }
}

