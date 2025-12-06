/**
 * CoordinationDecisionLogger - Captures complete decision context for training and debugging
 *
 * Implements Phase 5C: Decision logging in PersonaUser
 *
 * Every decision (RESPOND/SILENT) is logged with:
 * - Complete RAG context (what the LLM saw)
 * - Coordination snapshot (ThoughtStream state)
 * - Ambient state (temperature, user presence)
 * - Visual context (UI state)
 * - Decision details (action, confidence, reasoning)
 *
 * Enables:
 * - Time-travel debugging (replay decisions with different personas)
 * - Autopilot training (learn user's decision patterns)
 * - Meta-learning (companion AI suggestions → training data)
 */

import * as path from 'path';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { Commands } from '../../core/shared/Commands';
import type { DataCreateParams } from '../../../commands/data/create/shared/DataCreateTypes';
import { COLLECTIONS } from '../../data/config/DatabaseConfig';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import type {
  CoordinationDecisionEntity,
  DecisionAction,
  RAGContext,
  CoordinationSnapshot,
  AmbientState,
  VisualContext,
  DecisionData,
  DecisionMetadata,
  ChatUIVisualContext
} from '../../data/entities/CoordinationDecisionEntity';
import { getChatCoordinator } from './ChatCoordinationStream';
import { generateUUID } from '../../core/types/CrossPlatformUUID';
import { Logger, FileMode, type ComponentLogger } from '../../core/logging/Logger';
import { SystemPaths } from '../../core/config/SystemPaths';

/**
 * Parameters for logging a decision
 */
export interface LogDecisionParams {
  // Identity
  actorId: UUID;
  actorName: string;
  actorType: 'human' | 'ai-persona';
  triggerEventId: UUID;  // messageId for chat

  // Complete RAG context
  ragContext: RAGContext;

  // Visual context (optional, domain-specific)
  visualContext?: VisualContext;

  // Decision
  action: DecisionAction;
  confidence: number;
  reasoning?: string;
  responseContent?: string;
  modelUsed?: string;
  modelProvider?: string;
  tokensUsed?: number;
  responseTime: number;

  // Metadata
  sessionId: UUID;
  contextId: UUID;  // roomId for chat
  sequenceNumber?: number;
  tags?: string[];
}

/**
 * CoordinationDecisionLogger - Static utility for logging decisions
 */
export class CoordinationDecisionLogger {
  private static sequenceCounters = new Map<UUID, number>();
  private static logger: ComponentLogger = Logger.createWithFile(
    'CoordinationDecisionLogger',
    path.join(SystemPaths.logs.system, 'coordination-decisions.log'),
    FileMode.CLEAN
  );

  /**
   * Log a coordination decision to the database
   * Called at decision points in PersonaUser
   */
  static async logDecision(params: LogDecisionParams): Promise<void> {
    try {
      // Get sequence number for this actor
      const currentSeq = this.sequenceCounters.get(params.actorId) ?? 0;
      const sequenceNumber = params.sequenceNumber ?? currentSeq + 1;
      this.sequenceCounters.set(params.actorId, sequenceNumber);

      // Fetch coordination snapshot from ChatCoordinator
      const coordinator = getChatCoordinator();
      const stream = coordinator.getChatStream(params.triggerEventId);

      const coordinationSnapshot: CoordinationSnapshot = {
        thoughtStreamId: stream?.eventId,
        phase: (stream?.phase === 'deliberating' || stream?.phase === 'decided') ? 'deciding' : (stream?.phase ?? 'closed'),
        availableSlots: stream?.availableSlots ?? 0,
        myThought: stream ? {
          confidence: params.confidence,
          priority: params.confidence,  // Use confidence as priority
          timestamp: Date.now()
        } : undefined,
        competingThoughts: stream?.thoughts.map(thought => ({
          actorId: thought.personaId,
          actorName: thought.personaName,
          confidence: thought.confidence,
          priority: thought.confidence
        })) ?? [],
        othersConsideringCount: stream?.considerations.size ?? 0,
        othersConsideringNames: stream ? Array.from(stream.considerations.values()).map(t => t.personaName) : []
      };

      // Fetch ambient state from ChatCoordinator
      const ambientState: AmbientState = {
        temperature: coordinator.getTemperature(params.contextId),
        userPresent: coordinator.isUserPresent(params.contextId),
        timeSinceLastResponse: Date.now() - (stream?.startTime ?? Date.now()),
        messagesInLastMinute: 0,  // TODO: Calculate from stream activity
        mentionedByName: params.ragContext.conversationHistory.some(msg =>
          msg.content.toLowerCase().includes(`@${params.actorName.toLowerCase()}`)
        ),
        pressure: undefined  // Future: queue depth
      };

      // Build metadata
      const metadata: DecisionMetadata = {
        sessionId: params.sessionId,
        contextId: params.contextId,
        sequenceNumber,
        tags: params.tags ?? [],
        systemVersion: '0.1.0'  // TODO: Get from Constants
      };

      // Build decision data
      const decision: DecisionData = {
        action: params.action,
        confidence: params.confidence,
        reasoning: params.reasoning,
        responseContent: params.responseContent,
        modelUsed: params.modelUsed,
        modelProvider: params.modelProvider,
        tokensUsed: params.tokensUsed,
        responseTime: params.responseTime
        // companionSuggestion: undefined  // Future: Phase 5D meta-learning
      };

      // Create the entity
      const entity = {
        id: generateUUID(),
        timestamp: new Date(),
        createdAt: new Date(),
        updatedAt: new Date(),
        version: 0,

        // Identity
        actorId: params.actorId,
        actorType: params.actorType,
        actorName: params.actorName,
        triggerEventId: params.triggerEventId,
        domain: 'chat' as const,

        // Complete context
        ragContext: params.ragContext,
        visualContext: params.visualContext,
        coordinationSnapshot,
        ambientState,

        // Decision
        decision,

        // Outcome (nullable - rated later)
        outcome: undefined,

        // Metadata
        metadata
      } as any;

      // Store to database (fire-and-forget in PersonaUser, but sync here for error handling)
      await Commands.execute('data/create', {
        collection: COLLECTIONS.COORDINATION_DECISIONS,
        data: entity,
        backend: 'server',
        context: DataDaemon.jtagContext!,
        sessionId: DataDaemon.jtagContext!.uuid
      });

      this.logger.info(`📊 CoordinationDecisionLogger: Logged ${params.action} decision (seq=${sequenceNumber}, conf=${params.confidence.toFixed(2)}, temp=${ambientState.temperature.toFixed(2)})`);
    } catch (error) {
      this.logger.error(`❌ CoordinationDecisionLogger: Failed to log decision:`, error);
      // Don't throw - logging failures shouldn't break persona functionality
    }
  }

  /**
   * Build RAG context from conversation history
   * Used by PersonaUser to capture what the LLM saw
   */
  static buildRAGContext(
    systemPrompt: string,
    bio: string,
    role: string,
    conversationHistory: Array<{ role: 'system' | 'user' | 'assistant'; content: string; timestamp: number }>,
    artifacts?: Array<{ type: 'image' | 'file' | 'code'; name: string; content: string; mimeType?: string }>
  ): RAGContext {
    return {
      identity: {
        systemPrompt,
        bio,
        role
      },
      conversationHistory,
      artifacts,
      metadata: {
        timestamp: Date.now(),
        tokenCount: conversationHistory.reduce((sum, msg) => sum + msg.content.length / 4, 0),  // Rough estimate
        contextWindow: 4096  // TODO: Get from model config
      }
    };
  }

  /**
   * Build chat UI visual context
   * Captures what the persona sees in the UI
   */
  static buildChatVisualContext(
    visibleMessages: Array<{ id: UUID; senderId: UUID; content: string; timestamp: number }>,
    scrollPosition?: number,
    activeTab?: string
  ): ChatUIVisualContext {
    return {
      type: 'chat-ui',
      visibleMessages,
      scrollPosition,
      activeTab
    };
  }
}
