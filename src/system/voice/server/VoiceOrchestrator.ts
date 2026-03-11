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
import { VoiceSessionTimeline } from './VoiceSessionTimeline';
import { getRustVoiceOrchestrator } from './VoiceOrchestratorRustBridge';
import { BackpressureService } from '../../core/services/BackpressureService';
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
 * Session state — timeline + room mapping.
 * VoiceSessionTimeline handles all utterance tracking, cursors, and consolidation.
 */
interface SessionState {
  roomId: UUID;
  timeline: VoiceSessionTimeline;
}

/**
 * VoiceOrchestrator - Central coordinator for voice ↔ persona integration
 */
export class VoiceOrchestrator {
  private static _instance: VoiceOrchestrator | null = null;

  // Session state
  private sessionParticipants: Map<UUID, VoiceParticipant[]> = new Map();
  private sessions: Map<UUID, SessionState> = new Map();

  private constructor() {
    this.setupEventListeners();

    // Register with VoiceConversationSource for RAG context building
    registerVoiceOrchestrator(this);
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
            limit: participantIds.length,
            dbHandle: 'default'
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
      } catch {
        // Failed to load participants — continue with empty list
      }
    }

    this.sessionParticipants.set(sessionId, participants);
    this.sessions.set(sessionId, {
      roomId,
      timeline: new VoiceSessionTimeline(sessionId),
    });

    // Connect AI participants to the appropriate audio bridge
    const aiParticipants = participants.filter(p => p.type === 'persona' || p.type === 'agent');
    if (aiParticipants.length > 0) {
      const textBridge = getAIAudioBridge();
      const audioNativeBridge = getAudioNativeBridge();

      for (const ai of aiParticipants) {
        if (ai.isAudioNative && ai.modelId) {
          // Audio-native models: connect via AudioNativeBridge (direct audio I/O)
          audioNativeBridge.joinCall(sessionId, ai.userId, ai.displayName, ai.modelId).then(success => {
            if (!success) {
              // Audio-native connection failed — use text-based bridge
              textBridge.joinCall(sessionId, ai.userId, ai.displayName);
            }
          });
        } else {
          // Text-based models: connect via AIAudioBridge (STT -> LLM -> TTS)
          textBridge.joinCall(sessionId, ai.userId, ai.displayName);
        }
      }
    }
  }

  /**
   * Get the roomId for a voice session (for message persistence)
   */
  getRoomIdForSession(sessionId: UUID): UUID | null {
    return this.sessions.get(sessionId)?.roomId ?? null;
  }

  /**
   * Get the timeline for a session (for cursor-aware RAG access).
   * VoiceConversationSource uses this for per-persona temporal tracking.
   */
  getTimeline(sessionId: string): VoiceSessionTimeline | null {
    return this.sessions.get(sessionId as UUID)?.timeline ?? null;
  }

  /**
   * Unregister a voice session — cleans up TypeScript state AND tells Rust to
   * drop all LiveKit agents, Room listeners, and session state for this call.
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
    const session = this.sessions.get(sessionId);
    if (session) {
      session.timeline.clear();
      this.sessions.delete(sessionId);
    }

    // Tell Rust to drop LiveKit agents, Room listeners, and session state.
    // Fire-and-forget — TS cleanup above is already done.
    getRustVoiceOrchestrator().endSession(sessionId).catch(err => {
      console.error(`[VoiceOrchestrator] Rust session cleanup failed for ${sessionId}:`, err);
    });
  }

  /**
   * Handle incoming utterance from transcription
   *
   * Broadcasts to ALL text-based AI participants. No gating. No cooldowns.
   * No arbiter selection. Each AI decides independently whether to respond.
   */
  async onUtterance(event: UtteranceEvent): Promise<void> {
    const { sessionId, speakerId, transcript, speakerName } = event;

    const session = this.sessions.get(sessionId);
    if (!session) {
      return;
    }

    const participants = this.sessionParticipants.get(sessionId);
    if (!participants || participants.length === 0) {
      return;
    }

    // Ingest into timeline (handles consolidation + sequencing)
    session.timeline.append(event);

    // Get TEXT-BASED AI participants (excluding speaker AND audio-native AIs)
    // Audio-native AIs hear raw audio through mixer — text would cause double response
    const textAIs = participants.filter(
      p => p.type === 'persona' && p.userId !== speakerId && !p.isAudioNative
    );

    if (textAIs.length === 0) {
      return;
    }

    // Pressure-gated voice broadcast limiting
    const pressure = BackpressureService.pressureLevel;
    if (pressure === 'critical') return; // No broadcasts — let in-progress TTS finish

    let broadcastTargets = textAIs;
    if (pressure === 'high') {
      broadcastTargets = textAIs.slice(0, 1);  // Only first AI
    } else if (pressure === 'warning') {
      broadcastTargets = textAIs.slice(0, 3);  // Max 3
    }

    // Broadcast to pressure-limited set of text-based AIs
    for (const ai of broadcastTargets) {
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
   * Handle persona response — fire to Rust TTS with timeline seq for ordering.
   *
   * The seq number tells Rust WHERE in the conversation this response belongs.
   * Rust handles ordering, stale dropping, and TTS output scheduling internally —
   * no TS-side queuing overhead on the main thread.
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
      return;
    }

    // Attach the timeline seq this response is answering.
    // Rust uses this for output ordering and stale response detection.
    const timeline = this.getTimeline(sessionId);
    const respondingToSeq = timeline?.headSeq ?? 0;

    // Fire to Rust — TTS synthesis + ordering + LiveKit publish all in-process.
    // No TS scheduling overhead. Seq travels through IPC for Rust-side ordering.
    await bridge.speak(sessionId, personaId, response, undefined, respondingToSeq);
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

    // Listen for transcriptions — update session context for RAG only.
    // AI notification is handled by CollaborationLiveTranscriptionServerCommand
    // which calls the Rust VoiceOrchestrator and emits directed events.
    // We do NOT call onUtterance() here to avoid duplicate broadcasts.
    Events.subscribe('voice:transcription', async (event: {
      sessionId: string;
      speakerId: string;
      speakerName: string;
      transcript: string;
      confidence: number;
      language: string;
      timestamp: number;
    }) => {
      // Ingest into timeline (consolidation + sequencing handled automatically)
      const session = this.sessions.get(event.sessionId as UUID);
      if (session) {
        session.timeline.append({
          sessionId: event.sessionId,
          speakerId: event.speakerId,
          speakerName: event.speakerName,
          speakerType: 'human',
          transcript: event.transcript,
          confidence: event.confidence,
          timestamp: event.timestamp,
        });
      }
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
            dbHandle: 'default',
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
              const success = await audioNativeBridge.joinCall(sessionId, participant.userId, participant.displayName, modelId);
              if (!success) {
                const textBridge = getAIAudioBridge();
                textBridge.joinCall(sessionId, participant.userId, participant.displayName);
              }
            } else {
              const textBridge = getAIAudioBridge();
              textBridge.joinCall(sessionId, participant.userId, participant.displayName);
            }
          }
        } catch {
          // Failed to add mid-call joiner
        }
      }
    });

    // Listen for AI speech events — record in session context for RAG and export
    Events.subscribe('voice:ai:speech', (event: {
      sessionId: string;
      speakerId: string;
      speakerName: string;
      text: string;
      audioDurationMs?: number;
      failed?: boolean;
      timestamp: number;
    }) => {
      if (event.failed) return;

      const session = this.sessions.get(event.sessionId as UUID);
      if (session) {
        // Determine speaker type from participants
        const participants = this.sessionParticipants.get(event.sessionId as UUID);
        const participant = participants?.find(p => p.userId === event.speakerId);
        const speakerType = participant?.type ?? 'persona';

        session.timeline.append({
          sessionId: event.sessionId,
          speakerId: event.speakerId,
          speakerName: event.speakerName,
          speakerType,
          transcript: event.text,
          confidence: 1.0,
          timestamp: event.timestamp,
        });
      }
    });
  }

  /**
   * Get session statistics
   */
  getSessionStats(sessionId: UUID): { participants: number; turnCount: number } | null {
    const participants = this.sessionParticipants.get(sessionId);
    const session = this.sessions.get(sessionId);

    if (!participants || !session) return null;

    return {
      participants: participants.length,
      turnCount: session.timeline.totalTurns,
    };
  }

  /**
   * Get recent utterances for a voice session (backwards-compatible).
   * Utterances are already consolidated with "..." continuity markers by the timeline.
   *
   * For cursor-aware access, use getTimeline() instead.
   */
  getRecentUtterances(sessionId: string, limit: number = 20): UtteranceEvent[] {
    const session = this.sessions.get(sessionId as UUID);
    if (!session) {
      return [];
    }

    // Timeline returns consolidated turns with "..." markers already applied
    return session.timeline.getRecentUtterances(limit) as UtteranceEvent[];
  }

  /**
   * Get the active session ID (first/only active session).
   * Returns null if no live session is active.
   */
  get activeSessionId(): UUID | null {
    const keys = [...this.sessions.keys()];
    return keys.length > 0 ? keys[0] : null;
  }

  /**
   * Get participants for a session
   */
  getParticipants(sessionId: string): VoiceParticipant[] {
    return this.sessionParticipants.get(sessionId as UUID) ?? [];
  }
}

