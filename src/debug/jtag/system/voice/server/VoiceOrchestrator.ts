/**
 * VoiceOrchestrator - Bridges voice transcriptions with persona system
 *
 * Responsibilities:
 * 1. Receive transcription events from VoiceWebSocketHandler
 * 2. Post transcripts to chat (all AIs see them through normal flow)
 * 3. Perform turn arbitration (which AI responds via VOICE)
 * 4. Track pending voice responses
 * 5. Route responses to TTS when they come back
 *
 * Key Insight: Voice is a MODALITY, not a domain.
 * Personas see voice transcripts as chat messages - the sourceModality metadata
 * tells PersonaResponseGenerator to route the response to TTS instead of just posting.
 *
 * The orchestrator doesn't directly access PersonaInbox. Instead:
 * - Transcripts become chat messages (all AIs see them)
 * - Arbitration selects ONE responder for voice output
 * - Selected responder's response gets TTS routing via metadata
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { InboxMessage } from '../../user/server/modules/QueueItemTypes';
import { Events } from '../../core/shared/Events';
import { Commands } from '../../core/shared/Commands';
import type { UserEntity } from '../../data/entities/UserEntity';
import { generateUUID } from '../../core/types/CrossPlatformUUID';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import { DATA_COMMANDS } from '../../../commands/data/shared/DataCommandConstants';
import type { ChatSendParams, ChatSendResult } from '../../../commands/collaboration/chat/send/shared/ChatSendTypes';
import { getAIAudioBridge } from './AIAudioBridge';

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
}

/**
 * Turn arbitration strategy interface
 */
interface TurnArbiter {
  selectResponder(
    event: UtteranceEvent,
    candidates: VoiceParticipant[],
    context: ConversationContext
  ): VoiceParticipant | null;
}

/**
 * Conversation context for arbitration
 */
interface ConversationContext {
  sessionId: UUID;
  recentUtterances: UtteranceEvent[];
  lastResponderId?: UUID;
  turnCount: number;
}

/**
 * Pending response waiting for TTS
 */
interface PendingVoiceResponse {
  sessionId: UUID;
  personaId: UUID;
  originalMessageId: UUID;
  timestamp: number;
}

/**
 * VoiceOrchestrator - Central coordinator for voice ↔ persona integration
 */
export class VoiceOrchestrator {
  private static _instance: VoiceOrchestrator | null = null;

  // Session state
  private sessionParticipants: Map<UUID, VoiceParticipant[]> = new Map();
  private sessionContexts: Map<UUID, ConversationContext> = new Map();
  private pendingResponses: Map<UUID, PendingVoiceResponse> = new Map();

  // Turn arbitration
  private arbiter: TurnArbiter;

  // TTS callback (set by VoiceWebSocketHandler)
  private ttsCallback: ((sessionId: UUID, personaId: UUID, text: string) => Promise<void>) | null = null;

  private constructor() {
    this.arbiter = new CompositeArbiter();
    this.setupEventListeners();
    console.log('🎙️ VoiceOrchestrator: Initialized');
  }

  static get instance(): VoiceOrchestrator {
    if (!VoiceOrchestrator._instance) {
      VoiceOrchestrator._instance = new VoiceOrchestrator();
    }
    return VoiceOrchestrator._instance;
  }

  /**
   * Set the TTS callback for routing voice responses
   */
  setTTSCallback(callback: (sessionId: UUID, personaId: UUID, text: string) => Promise<void>): void {
    this.ttsCallback = callback;
  }

  /**
   * Register participants for a voice session
   */
  async registerSession(sessionId: UUID, participantIds: UUID[]): Promise<void> {
    const participants: VoiceParticipant[] = [];

    // Look up users from database
    if (participantIds.length > 0) {
      try {
        const result = await Commands.execute<DataListParams, DataListResult<UserEntity>>(
          DATA_COMMANDS.LIST,
          {
            collection: 'users',
            filter: { id: { $in: participantIds } },
            limit: participantIds.length
          }
        );

        if (result.success && result.items) {
          for (const user of result.items) {
            participants.push({
              userId: user.id as UUID,
              displayName: user.displayName || user.uniqueId,
              type: user.type as 'human' | 'persona' | 'agent',
              expertise: (user.metadata as Record<string, unknown>)?.expertise as string[] | undefined
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
      recentUtterances: [],
      turnCount: 0
    });

    console.log(`🎙️ VoiceOrchestrator: Registered session ${sessionId.slice(0, 8)} with ${participants.length} participants`);

    // Connect AI participants to the audio call server
    const aiParticipants = participants.filter(p => p.type === 'persona' || p.type === 'agent');
    if (aiParticipants.length > 0) {
      const bridge = getAIAudioBridge();
      for (const ai of aiParticipants) {
        console.log(`🎙️ VoiceOrchestrator: Connecting ${ai.displayName} to audio call...`);
        bridge.joinCall(sessionId, ai.userId, ai.displayName).then(success => {
          if (success) {
            console.log(`🎙️ VoiceOrchestrator: ${ai.displayName} connected to audio`);
          } else {
            console.warn(`🎙️ VoiceOrchestrator: ${ai.displayName} failed to connect to audio`);
          }
        });
      }
    }
  }

  /**
   * Unregister a voice session
   */
  unregisterSession(sessionId: UUID): void {
    // Disconnect AI participants from audio call
    const participants = this.sessionParticipants.get(sessionId);
    if (participants) {
      const bridge = getAIAudioBridge();
      const aiParticipants = participants.filter(p => p.type === 'persona' || p.type === 'agent');
      for (const ai of aiParticipants) {
        bridge.leaveCall(sessionId, ai.userId);
      }
    }

    this.sessionParticipants.delete(sessionId);
    this.sessionContexts.delete(sessionId);
    console.log(`🎙️ VoiceOrchestrator: Unregistered session ${sessionId.slice(0, 8)}`);
  }

  /**
   * Handle incoming utterance from transcription
   *
   * This is the main entry point called by VoiceWebSocketHandler
   *
   * Key flow:
   * 1. Post transcript to chat (so ALL AIs see it, including text-only models)
   * 2. Turn arbitration selects ONE responder for voice output
   * 3. Selected responder gets InboxMessage with sourceModality='voice'
   * 4. All other AIs see the chat message normally (can respond via text)
   */
  async onUtterance(event: UtteranceEvent): Promise<void> {
    const { sessionId, speakerId, transcript, speakerName } = event;

    console.log(`🎙️ VoiceOrchestrator: Utterance from ${speakerName}: "${transcript.slice(0, 50)}..."`);

    // Step 1: Post transcript to chat room (visible to ALL AIs including text-only)
    // This ensures the conversation history is captured and all models can see it
    // Note: Voice metadata is tracked separately in pendingResponses for TTS routing
    try {
      await Commands.execute<ChatSendParams, ChatSendResult>('collaboration/chat/send', {
        room: sessionId,
        message: `[Voice] ${speakerName}: ${transcript}`
      });
    } catch (error) {
      console.warn('🎙️ VoiceOrchestrator: Failed to post transcript to chat:', error);
    }

    // Get participants for this session
    const participants = this.sessionParticipants.get(sessionId);
    if (!participants || participants.length === 0) {
      console.warn(`🎙️ VoiceOrchestrator: No participants registered for session ${sessionId.slice(0, 8)}`);
      return;
    }

    // Get conversation context
    const context = this.sessionContexts.get(sessionId);
    if (!context) {
      console.warn(`🎙️ VoiceOrchestrator: No context for session ${sessionId.slice(0, 8)}`);
      return;
    }

    // Update context with new utterance
    context.recentUtterances.push(event);
    if (context.recentUtterances.length > 20) {
      context.recentUtterances.shift(); // Keep last 20
    }
    context.turnCount++;

    // Get AI participants (excluding the speaker)
    const aiParticipants = participants.filter(
      p => p.type === 'persona' && p.userId !== speakerId
    );

    if (aiParticipants.length === 0) {
      console.log('🎙️ VoiceOrchestrator: No AI participants to respond');
      return;
    }

    // Step 2: Turn arbitration - which AI responds via VOICE?
    // Other AIs will see the chat message and may respond via text
    const responder = this.arbiter.selectResponder(event, aiParticipants, context);

    if (!responder) {
      console.log('🎙️ VoiceOrchestrator: Arbiter selected no voice responder (AIs may still respond via text)');
      return;
    }

    console.log(`🎙️ VoiceOrchestrator: ${responder.displayName} selected to respond via voice`);

    // Step 3: Track who should respond via voice
    // The persona will see the chat message through their normal inbox polling
    // When they respond, we'll intercept it for TTS via event subscription
    const pendingId = generateUUID();
    this.pendingResponses.set(pendingId, {
      sessionId,
      personaId: responder.userId,
      originalMessageId: pendingId,
      timestamp: Date.now()
    });

    // Track selected responder for this session
    // When this persona posts a message to this room, route to TTS
    this.trackVoiceResponder(sessionId, responder.userId);

    // Update last responder
    context.lastResponderId = responder.userId;
  }

  /**
   * Track which persona should respond via voice for a session
   */
  private voiceResponders: Map<UUID, UUID> = new Map();  // sessionId -> personaId

  private trackVoiceResponder(sessionId: UUID, personaId: UUID): void {
    this.voiceResponders.set(sessionId, personaId);
    console.log(`🎙️ VoiceOrchestrator: Tracking ${personaId.slice(0, 8)} as voice responder for session ${sessionId.slice(0, 8)}`);
  }

  /**
   * Check if a persona's response should be routed to TTS
   */
  shouldRouteToTTS(sessionId: UUID, personaId: UUID): boolean {
    const expectedResponder = this.voiceResponders.get(sessionId);
    return expectedResponder === personaId;
  }

  /**
   * Clear voice responder after they respond (one response per utterance)
   */
  clearVoiceResponder(sessionId: UUID): void {
    this.voiceResponders.delete(sessionId);
  }

  /**
   * Handle persona response (called from PersonaResponseGenerator)
   *
   * When a persona generates a response to a voice message,
   * this routes it to TTS instead of posting as text.
   */
  async onPersonaResponse(
    personaId: UUID,
    response: string,
    originalMessage: InboxMessage
  ): Promise<void> {
    // Only handle voice messages
    if (originalMessage.sourceModality !== 'voice' || !originalMessage.voiceSessionId) {
      return;
    }

    const sessionId = originalMessage.voiceSessionId;

    console.log(`🎙️ VoiceOrchestrator: Routing response to TTS for session ${sessionId.slice(0, 8)}`);

    // Clean up pending response
    this.pendingResponses.delete(originalMessage.id);

    // Route to TTS via AIAudioBridge (injects audio into the call)
    const bridge = getAIAudioBridge();
    if (bridge.isInCall(sessionId, personaId)) {
      await bridge.speak(sessionId, personaId, response);
    } else if (this.ttsCallback) {
      // Fallback to external TTS callback if set
      await this.ttsCallback(sessionId, personaId, response);
    } else {
      console.warn('🎙️ VoiceOrchestrator: AI not in call and no TTS callback');
    }
  }

  /**
   * Check if a message should be routed to voice
   */
  isVoiceMessage(message: InboxMessage): boolean {
    return message.sourceModality === 'voice' && !!message.voiceSessionId;
  }

  /**
   * Setup event listeners for persona responses
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
  }

  /**
   * Get session statistics
   */
  getSessionStats(sessionId: UUID): { participants: number; turnCount: number; pendingResponses: number } | null {
    const participants = this.sessionParticipants.get(sessionId);
    const context = this.sessionContexts.get(sessionId);

    if (!participants || !context) return null;

    const pendingCount = Array.from(this.pendingResponses.values())
      .filter(p => p.sessionId === sessionId).length;

    return {
      participants: participants.length,
      turnCount: context.turnCount,
      pendingResponses: pendingCount
    };
  }
}

// ============================================================================
// Turn Arbitration Strategies
// ============================================================================

/**
 * Directed addressing arbiter - responds when directly addressed
 */
class DirectedArbiter implements TurnArbiter {
  selectResponder(
    event: UtteranceEvent,
    candidates: VoiceParticipant[],
    _context: ConversationContext
  ): VoiceParticipant | null {
    const textLower = event.transcript.toLowerCase();

    // Check for direct address ("Hey Teacher", "Teacher, ...", "@Teacher")
    for (const candidate of candidates) {
      const nameLower = candidate.displayName.toLowerCase();
      const nameHyphen = nameLower.replace(/\s+/g, '-');

      if (textLower.includes(nameLower) ||
          textLower.includes(nameHyphen) ||
          textLower.includes(`@${nameLower}`) ||
          textLower.includes(`@${nameHyphen}`)) {
        return candidate;
      }
    }

    return null;
  }
}

/**
 * Topic relevance arbiter - responds based on expertise match
 */
class RelevanceArbiter implements TurnArbiter {
  selectResponder(
    event: UtteranceEvent,
    candidates: VoiceParticipant[],
    _context: ConversationContext
  ): VoiceParticipant | null {
    const textLower = event.transcript.toLowerCase();

    // Score each candidate by expertise match
    const scored = candidates.map(candidate => {
      let score = 0;

      if (candidate.expertise) {
        for (const keyword of candidate.expertise) {
          if (textLower.includes(keyword.toLowerCase())) {
            score += 0.3;
          }
        }
      }

      return { candidate, score };
    });

    // Sort by score descending
    scored.sort((a, b) => b.score - a.score);

    // Return best if score is meaningful
    if (scored.length > 0 && scored[0].score > 0.2) {
      return scored[0].candidate;
    }

    return null;
  }
}

/**
 * Round-robin arbiter - takes turns between AIs
 */
class RoundRobinArbiter implements TurnArbiter {
  selectResponder(
    _event: UtteranceEvent,
    candidates: VoiceParticipant[],
    context: ConversationContext
  ): VoiceParticipant | null {
    if (candidates.length === 0) return null;

    // Find next candidate after last responder
    if (context.lastResponderId) {
      const lastIndex = candidates.findIndex(c => c.userId === context.lastResponderId);
      if (lastIndex >= 0) {
        const nextIndex = (lastIndex + 1) % candidates.length;
        return candidates[nextIndex];
      }
    }

    // Default to first candidate
    return candidates[0];
  }
}

/**
 * Composite arbiter - tries directed first, then relevance, then round-robin
 */
class CompositeArbiter implements TurnArbiter {
  private directed = new DirectedArbiter();
  private relevance = new RelevanceArbiter();
  private roundRobin = new RoundRobinArbiter();

  selectResponder(
    event: UtteranceEvent,
    candidates: VoiceParticipant[],
    context: ConversationContext
  ): VoiceParticipant | null {
    // 1. Direct address takes precedence
    const directed = this.directed.selectResponder(event, candidates, context);
    if (directed) {
      console.log(`🎙️ Arbiter: Selected ${directed.displayName} (directed)`);
      return directed;
    }

    // 2. Topic relevance
    const relevant = this.relevance.selectResponder(event, candidates, context);
    if (relevant) {
      console.log(`🎙️ Arbiter: Selected ${relevant.displayName} (relevance)`);
      return relevant;
    }

    // 3. Fall back to round-robin (but only for questions)
    const isQuestion = event.transcript.includes('?') ||
                       event.transcript.toLowerCase().startsWith('what') ||
                       event.transcript.toLowerCase().startsWith('how') ||
                       event.transcript.toLowerCase().startsWith('why') ||
                       event.transcript.toLowerCase().startsWith('can') ||
                       event.transcript.toLowerCase().startsWith('could');

    if (isQuestion) {
      const next = this.roundRobin.selectResponder(event, candidates, context);
      if (next) {
        console.log(`🎙️ Arbiter: Selected ${next.displayName} (round-robin for question)`);
        return next;
      }
    }

    // 4. No one responds to statements (prevents spam)
    console.log('🎙️ Arbiter: No responder selected (statement, not question)');
    return null;
  }
}

// Export singleton accessor
export function getVoiceOrchestrator(): VoiceOrchestrator {
  return VoiceOrchestrator.instance;
}
