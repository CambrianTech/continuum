/**
 * PersonaMessageEvaluator - Handles message evaluation and response decision for PersonaUser
 *
 * REFACTORING: Extracted from PersonaUser.ts (lines 566-1869)
 * Pure function extraction - no behavioral changes
 *
 * This module contains the core message evaluation logic:
 * - Cognition-based response planning
 * - LLM-based gating decisions
 * - Heuristic fallbacks
 * - Response coordination
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { ORM } from '../../../../daemons/data-daemon/server/ORM';
import { inspect } from 'util';
import { Events } from '../../../core/shared/Events';
import { COLLECTIONS } from '../../../shared/Constants';
import type { ChatMessageEntity } from '../../../data/entities/ChatMessageEntity';
import type { ProcessableMessage } from './QueueItemTypes';
// UserEntity and RoomEntity imports removed — isSenderHuman() moved to Rust
import { CognitionLogger } from './cognition/CognitionLogger';
import { SignalDetector, getSignalDetector } from './SignalDetector';
import { getTrainingBuffer } from './TrainingBuffer';
import type { Task } from './cognition/reasoning/types';
import { ChatRAGBuilder } from '../../../rag/builders/ChatRAGBuilder';
import { getToolCapability } from './ToolFormatAdapter';
import { CoordinationDecisionLogger, type LogDecisionParams } from '../../../coordination/server/CoordinationDecisionLogger';
import type { RAGContext } from '../../../data/entities/CoordinationDecisionEntity';
import type { RAGContext as PipelineRAGContext, RAGArtifact } from '../../../rag/shared/RAGTypes';
import { contentPreview, truncate } from '../../../../shared/utils/StringUtils';
import type { DecisionContext } from './cognition/adapters/IDecisionAdapter';
import { getChatCoordinator } from '../../../coordination/server/ChatCoordinationStream';
import { calculateMessagePriority } from './PersonaInbox';
import { toInboxMessageRequest } from './RustCognitionBridge';
import type { SenderType, FullEvaluateResult } from '../../../../shared/generated';
import type { FastPathDecision } from './central-nervous-system/CNSTypes';
// personaSleepManager no longer needed — sleep mode gating moved to Rust evaluator
import {
  AI_DECISION_EVENTS,
  type AIEvaluatingEventData,
  type AIDecidedSilentEventData,
  type AIDecidedRespondEventData,
  type AIGeneratingEventData,
  type AIErrorEventData
} from '../../../events/shared/AIDecisionEvents';
import { EVENT_SCOPES } from '../../../events/shared/EventSystemConstants';
import {
  COGNITION_EVENTS,
  type StageCompleteEvent,
  calculateSpeedScore,
  getStageStatus
} from '../../../conversation/shared/CognitionEventTypes';

// Import PersonaUser directly - circular dependency is fine for type-only imports
import type { PersonaUser } from '../PersonaUser';

/**
 * Discriminated union for gating result.
 * When shouldRespond=true: full RAG context is guaranteed (built once, reused by generator).
 * When shouldRespond=false: no RAG context (skipped for performance).
 */
interface GatingResultBase {
  confidence: number;
  reason: string;
  model: string;
}

export interface GatingRespondResult extends GatingResultBase {
  shouldRespond: true;
  filteredRagContext: PipelineRAGContext;
  ragContextSummary: {
    totalMessages: number;
    filteredMessages: number;
    timeWindowMinutes: number;
  };
}

export interface GatingSilentResult extends GatingResultBase {
  shouldRespond: false;
}

export type GatingResult = GatingRespondResult | GatingSilentResult;

/**
 * PersonaMessageEvaluator - Message evaluation and response decision engine
 *
 * Handles:
 * - Cognition-based response planning (with SelfState, WorkingMemory)
 * - Message gating (should respond?)
 * - Response coordination (with other AIs)
 * - Heuristic scoring and fallbacks
 */
export class PersonaMessageEvaluator {
  private readonly signalDetector: SignalDetector;

  // In-memory recent message cache — eliminates SQLite queries for post-inference validation.
  // Populated by event subscription on first use. Bounded to last 50 messages per room.
  private static _recentMessages: Map<string, ChatMessageEntity[]> = new Map();
  private static _cacheInitialized = false;
  private static readonly MAX_CACHED_PER_ROOM = 50;

  private static initMessageCache(): void {
    if (PersonaMessageEvaluator._cacheInitialized) return;
    PersonaMessageEvaluator._cacheInitialized = true;

    Events.subscribe(`data:${COLLECTIONS.CHAT_MESSAGES}:created`, (entity: any) => {
      const msg = entity as ChatMessageEntity;
      if (!msg.roomId) return;
      const roomId = msg.roomId;
      let messages = PersonaMessageEvaluator._recentMessages.get(roomId);
      if (!messages) {
        messages = [];
        PersonaMessageEvaluator._recentMessages.set(roomId, messages);
      }
      messages.push(msg);
      if (messages.length > PersonaMessageEvaluator.MAX_CACHED_PER_ROOM) {
        messages.shift();
      }
    });
  }

  /**
   * Get recent messages for a room from in-memory cache, filtered by timestamp.
   * Returns flat ChatMessageEntity objects (not DataRecord-wrapped).
   */
  private static getRecentMessagesSince(roomId: UUID, since: Date): ChatMessageEntity[] {
    PersonaMessageEvaluator.initMessageCache();
    const messages = PersonaMessageEvaluator._recentMessages.get(roomId);
    if (!messages) return [];
    const sinceTime = since.getTime();
    return messages.filter(m => {
      const ts = m.timestamp instanceof Date ? m.timestamp.getTime() : new Date(m.timestamp).getTime();
      return ts > sinceTime;
    });
  }

  constructor(private readonly personaUser: PersonaUser) {
    this.signalDetector = getSignalDetector();
    // Ensure cache is initialized on first evaluator creation
    PersonaMessageEvaluator.initMessageCache();
  }

  /**
   * Detect training signals from human messages and add to training buffer
   *
   * Part of continuous micro-LoRA: when a human corrects or approves an AI response,
   * we capture that as a training signal for the appropriate trait adapter.
   */
  private async detectAndBufferTrainingSignal(
    messageEntity: ProcessableMessage
  ): Promise<void> {
    // Signal detection focuses on MESSAGE CONTENT, not sender type
    // The AI classifier determines if it's feedback based on what's written
    try {
      // Get the preceding AI message (if any)
      const precedingAIMessage = await this.getPrecedingAIMessage(messageEntity);

      // Get recent conversation history for context
      const conversationHistory = await this.getRecentConversationHistory(messageEntity.roomId);

      // Use AI-powered signal detection (async) for semantic classification
      const signal = await this.signalDetector.detectSignalAsync(
        messageEntity,
        precedingAIMessage,
        conversationHistory
      );

      if (signal && signal.type !== 'none') {
        this.log(`🎓 ${this.personaUser.displayName}: Training signal detected via AI classification`);
        this.log(`   Type: ${signal.type}, Trait: ${signal.trait}, Polarity: ${signal.polarity}`);
        this.log(`   Confidence: ${(signal.confidence * 100).toFixed(0)}%`);

        // Add to training buffer with persona-specific logger
        const trainingLogger = (msg: string) => this.log(`[TrainingBuffer] ${msg}`);
        const buffer = getTrainingBuffer(this.personaUser.id, this.personaUser.displayName, trainingLogger);
        const trainingTriggered = await buffer.add(signal);

        if (trainingTriggered) {
          this.log(`🔥 ${this.personaUser.displayName}: Micro-training triggered for ${signal.trait}!`);
        }
      }
    } catch (error) {
      // Don't let signal detection failures block message processing
      this.log(`⚠️ ${this.personaUser.displayName}: Signal detection error (non-fatal):`, error);
    }
  }

  /**
   * Get the preceding AI message before a human message (for correction detection)
   */
  private async getPrecedingAIMessage(humanMessage: ProcessableMessage): Promise<ChatMessageEntity | null> {
    try {
      const result = await ORM.query<ChatMessageEntity>({
        collection: COLLECTIONS.CHAT_MESSAGES,
        filter: {
          roomId: humanMessage.roomId,
          timestamp: { $lt: humanMessage.timestamp },
          senderType: { $ne: 'human' }  // AI or persona
        },
        sort: [{ field: 'timestamp', direction: 'desc' }],
        limit: 1
      });

      if (result.success && result.data && result.data.length > 0) {
        // Only return if it's from THIS persona (we're learning from our own corrections)
        const msg = result.data[0].data;
        if (msg.senderId === this.personaUser.id) {
          return msg;
        }
      }

      return null;
    } catch {
      return null;
    }
  }

  /**
   * Get recent conversation history for training context
   */
  private async getRecentConversationHistory(roomId: UUID, limit: number = 10): Promise<ChatMessageEntity[]> {
    try {
      const result = await ORM.query<ChatMessageEntity>({
        collection: COLLECTIONS.CHAT_MESSAGES,
        filter: { roomId },
        sort: [{ field: 'timestamp', direction: 'desc' }],
        limit
      });

      if (result.success && result.data) {
        return result.data.map(record => record.data).reverse();  // Chronological order
      }

      return [];
    } catch {
      return [];
    }
  }

  /**
   * Log to persona's cognition.log file
   */
  private log(message: string, ...args: any[]): void {
    const timestamp = new Date().toISOString();
    const formattedArgs = args.length > 0
      ? ' ' + args.map(a =>
          typeof a === 'object' ? inspect(a, { depth: 2, colors: false, compact: true }) : String(a)
        ).join(' ')
      : '';
    this.personaUser.logger.enqueueLog('cognition.log', `[${timestamp}] ${message}${formattedArgs}\n`);
  }

  /**
   * Evaluate message with full cognition system (planning, focus, working memory)
   *
   * PHASE 4: Cognition wrapper around evaluateAndPossiblyRespond
   * Creates Task → Plan → Updates SelfState → Executes → Logs to CognitionLogger
   */
  async evaluateAndPossiblyRespondWithCognition(
    messageEntity: ProcessableMessage,
    senderIsHuman: boolean,
    messageText: string,
    preComputedDecision?: FastPathDecision
  ): Promise<void> {
    // Defensive: ensure messageText is always a string (prevents slice errors)
    const safeMessageText = messageText ?? '';
    const taskStartTime = Date.now();
    // Evaluator pipeline timing — tracks every phase before generation
    const evalTiming: Record<string, number> = {};

    // EARLY GATE: Unified evaluation — ALL pre-response gates in ONE Rust IPC call.
    // Replaces: response_cap → mention → rate_limit → sleep_mode → directed_mention → fast_path
    // Must run BEFORE expensive cognition work (plan formulation, working memory, state snapshots).
    const earlyGateStart = Date.now();
    const earlyResult = await this.personaUser.rustCognition.fullEvaluate({
      persona_id: this.personaUser.id,
      persona_name: this.personaUser.displayName,
      persona_unique_id: this.personaUser.entity?.uniqueId ?? '',
      message_id: messageEntity.id,
      room_id: messageEntity.roomId,
      sender_id: messageEntity.senderId,
      sender_name: messageEntity.senderName,
      sender_type: messageEntity.senderType as SenderType ?? (senderIsHuman ? 'human' : 'persona'),
      content: safeMessageText,
      timestamp: this.personaUser.timestampToNumber(messageEntity.timestamp),
      is_voice: false,
      sender_is_human: senderIsHuman,
    });
    evalTiming['early_gate'] = Date.now() - earlyGateStart;

    if (!earlyResult.should_respond) {
      this.log(`🚫 ${this.personaUser.displayName}: Early gate SILENT — gate=${earlyResult.gate}, reason="${earlyResult.reason}" (${earlyResult.decision_time_ms.toFixed(2)}ms)`);
      this.personaUser.logAIDecision('SILENT', `${earlyResult.gate}: ${earlyResult.reason}`, {
        message: safeMessageText.slice(0, 100),
        sender: messageEntity.senderName,
        roomId: messageEntity.roomId,
      });
      return;
    }

    // SIGNAL DETECTION: Analyze message content for training signals
    // Fire-and-forget - AI classifier determines if content is feedback
    this.detectAndBufferTrainingSignal(messageEntity).catch(err => {
      this.log(`⚠️ ${this.personaUser.displayName}: Signal detection failed (non-fatal):`, err);
    });

    // STEP 1: Create Task from message
    let t0 = Date.now();
    const task: Task = {
      id: `task-${messageEntity.id}` as UUID,
      domain: 'chat',
      contextId: messageEntity.roomId,
      description: `Respond to: "${safeMessageText.slice(0, 100)}"`,
      priority: calculateMessagePriority(
        {
          content: safeMessageText,
          timestamp: this.personaUser.timestampToNumber(messageEntity.timestamp),
          roomId: messageEntity.roomId
        },
        {
          displayName: this.personaUser.displayName,
          id: this.personaUser.id,
          recentRooms: Array.from(this.personaUser.myRoomIds)
        }
      ),
      triggeredBy: messageEntity.senderId,
      createdAt: Date.now()
    };
    evalTiming['task_create'] = Date.now() - t0;

    this.log(`🧠 ${this.personaUser.displayName}: COGNITION - Created task for message from ${messageEntity.senderName}`);

    // STEP 2: Generate Plan
    t0 = Date.now();
    const plan = await this.personaUser.planFormulator.formulatePlan(task);
    evalTiming['plan_formulate'] = Date.now() - t0;
    this.log(`📋 ${this.personaUser.displayName}: COGNITION - Plan: ${plan.goal} (${evalTiming['plan_formulate']}ms)`);
    this.log(`   Steps: ${plan.steps.map((s: any) => s.action).join(' → ')}`);

    // LOG: Plan formulation (fire-and-forget — no longer blocks pipeline)
    t0 = Date.now();
    CognitionLogger.logPlanFormulation(
      this.personaUser.id,
      this.personaUser.displayName,
      task,
      plan,
      'chat',
      messageEntity.roomId,
      'template-based'  // SimplePlanFormulator uses templates
    );
    evalTiming['plan_log'] = Date.now() - t0;

    // STEP 3: Update SelfState - set focus
    t0 = Date.now();
    await this.personaUser.selfState.updateFocus({
      activity: 'chat-response',
      objective: plan.goal,
      intensity: task.priority
    });
    await this.personaUser.selfState.updateLoad(0.2); // Chat response adds cognitive load
    evalTiming['state_update'] = Date.now() - t0;

    // LOG: State snapshot after focus/load update
    t0 = Date.now();
    const selfState = await this.personaUser.selfState.get();
    const workingMemoryEntries = await this.personaUser.workingMemory.recall({
      domain: 'chat',
      contextId: messageEntity.roomId,
      limit: 100
    });
    const capacity = await this.personaUser.workingMemory.getCapacity('chat');

    CognitionLogger.logStateSnapshot(
      this.personaUser.id,
      this.personaUser.displayName,
      selfState,
      workingMemoryEntries,
      {
        used: capacity.used,
        max: capacity.max,
        byDomain: { chat: capacity.used }
      },
      {
        domain: 'chat',
        contextId: messageEntity.roomId,
        triggerEvent: 'message-received'
      }
    );
    evalTiming['state_snapshot'] = Date.now() - t0;

    // STEP 4: Store initial observation in WorkingMemory
    t0 = Date.now();
    await this.personaUser.workingMemory.store({
      domain: 'chat',
      contextId: messageEntity.roomId,
      thoughtType: 'observation',
      thoughtContent: `Received message from ${messageEntity.senderName}: "${safeMessageText.slice(0, 200)}"`,
      importance: task.priority,
      shareable: false
    });
    evalTiming['wm_store_observation'] = Date.now() - t0;

    // STEP 5: Execute plan steps (existing chat logic inside)
    try {
      // Mark step 1 complete: "Recall relevant context"
      plan.steps[0].completed = true;
      plan.steps[0].completedAt = Date.now();

      // Execute step 2: "Generate thoughtful response" (existing logic)
      t0 = Date.now();
      await this.evaluateAndPossiblyRespond(messageEntity, senderIsHuman, safeMessageText, preComputedDecision);
      evalTiming['evaluate_and_respond'] = Date.now() - t0;

      // If we got here, response was generated (or decision was SILENT)
      plan.steps[1].completed = true;
      plan.steps[1].completedAt = Date.now();

      // Note: Step 3 "Post message" happens inside evaluateAndPossiblyRespond if decision was RESPOND
      if (plan.steps.length > 2) {
        plan.steps[2].completed = true;
        plan.steps[2].completedAt = Date.now();
      }

      // STEP 6: Store outcome in WorkingMemory
      t0 = Date.now();
      await this.personaUser.workingMemory.store({
        domain: 'chat',
        contextId: messageEntity.roomId,
        thoughtType: 'reflection',
        thoughtContent: `Completed response plan for message from ${messageEntity.senderName}`,
        importance: 0.5,
        shareable: false
      });
      evalTiming['wm_store_reflection'] = Date.now() - t0;

      this.log(`✅ ${this.personaUser.displayName}: COGNITION - Plan completed successfully`);

      // LOG: Plan completion (fire-and-forget — no longer blocks pipeline)
      t0 = Date.now();
      CognitionLogger.logPlanCompletion(
        plan.id,
        'completed',
        plan.steps.map((s: any) => ({
          stepNumber: s.stepNumber,
          action: s.action,
          expectedOutcome: s.expectedOutcome,
          completed: s.completed,
          completedAt: s.completedAt,
          result: s.result
        }))
      );
      evalTiming['plan_completion_log'] = Date.now() - t0;
    } catch (error: any) {
      this.log(`❌ ${this.personaUser.displayName}: COGNITION - Plan execution failed:`, error);

      // Store error in WorkingMemory
      await this.personaUser.workingMemory.store({
        domain: 'chat',
        contextId: messageEntity.roomId,
        thoughtType: 'observation',
        thoughtContent: `Error during response: ${error instanceof Error ? error.message : 'Unknown error'}`,
        importance: 0.8, // High importance for errors
        shareable: false
      });

      // LOG: Plan failure (fire-and-forget — no longer blocks pipeline)
      CognitionLogger.logPlanCompletion(
        plan.id,
        'failed',
        plan.steps.map((s: any) => ({
          stepNumber: s.stepNumber,
          action: s.action,
          expectedOutcome: s.expectedOutcome,
          completed: s.completed,
          completedAt: s.completedAt,
          result: s.result,
          error: error instanceof Error ? error.message : 'Unknown error'
        }))
      );
    } finally {
      // STEP 7: Clear focus and reduce cognitive load
      t0 = Date.now();
      await this.personaUser.selfState.clearFocus();
      await this.personaUser.selfState.updateLoad(-0.2); // Remove the load we added
      evalTiming['state_cleanup'] = Date.now() - t0;

      const duration = Date.now() - taskStartTime;
      const phases = Object.entries(evalTiming)
        .map(([k, v]) => `${k}=${v}ms`)
        .join(' | ');
      this.log(`📊 ${this.personaUser.displayName}: [EVAL-PIPELINE] Total=${duration}ms | ${phases}`);
    }
  }

  /**
   * Evaluate message and possibly respond (called with exclusive evaluation lock)
   *
   * NOTE: Now called from evaluateAndPossiblyRespondWithCognition wrapper
   */
  async evaluateAndPossiblyRespond(
    messageEntity: ProcessableMessage,
    senderIsHuman: boolean,
    safeMessageText: string,
    preComputedDecision?: FastPathDecision
  ): Promise<void> {
    // ALL pre-response gates are now handled by Rust via fullEvaluate() in the
    // evaluateAndPossiblyRespondWithCognition() wrapper. By the time we get here,
    // the early gate already passed. We extract mention info from gate_details.
    const isMentioned = preComputedDecision
      ? true  // If pre-computed, Rust already verified we should respond
      : false; // Default — the early gate already filtered directed mentions

    // === EVALUATE: Use LLM-based intelligent gating to decide if should respond ===
    // Emit EVALUATING event for real-time feedback (fire-and-forget — UI indicator)
    if (this.personaUser.client) {
      Events.emit<AIEvaluatingEventData>(
        DataDaemon.jtagContext!,
        AI_DECISION_EVENTS.EVALUATING,
        {
          personaId: this.personaUser.id,
          personaName: this.personaUser.displayName,
          roomId: messageEntity.roomId,
          messageId: messageEntity.id,
          isHumanMessage: senderIsHuman,
          timestamp: Date.now(),
          messagePreview: safeMessageText.slice(0, 100),
          senderName: messageEntity.senderName
        },
        {
          scope: EVENT_SCOPES.ROOM,
          scopeId: messageEntity.roomId,
        }
      ).catch(err => this.log(`⚠️ Event emit failed: ${err}`));
    }

    const gatingStart = Date.now();
    const gatingResult = await this.evaluateShouldRespond(messageEntity, senderIsHuman, isMentioned, preComputedDecision);
    this.log(`⏱️ ${this.personaUser.displayName}: [INNER] evaluateShouldRespond=${Date.now() - gatingStart}ms`);

    // FULL TRANSPARENCY LOGGING
    this.log(`\n${'='.repeat(80)}`);
    this.log(`🧠 ${this.personaUser.displayName}: GATING DECISION for message "${safeMessageText.slice(0, 60)}..."`);
    this.log(`${'='.repeat(80)}`);
    if (gatingResult.shouldRespond) {
      this.log(`📊 Context: ${gatingResult.ragContextSummary.filteredMessages} messages in ${gatingResult.ragContextSummary.timeWindowMinutes}min window`);
      this.log(`💬 Conversation history (last 5):`);
      gatingResult.filteredRagContext.conversationHistory.slice(-5).forEach((msg, i) => {
        this.log(`   ${i + 1}. [${msg.name ?? msg.role}] ${truncate(msg.content, 80)}...`);
      });
    }
    this.log(`\n🎯 Decision: ${gatingResult.shouldRespond ? 'RESPOND' : 'SILENT'}`);
    this.log(`   Confidence: ${(gatingResult.confidence * 100).toFixed(0)}%`);
    this.log(`   Reason: ${gatingResult.reason}`);
    this.log(`   Model: ${gatingResult.model}`);
    this.log(`${'='.repeat(80)}\n`);

    if (!gatingResult.shouldRespond) {
      // SILENT: No RAG context available (skipped for performance)
      this.personaUser.logAIDecision('SILENT', gatingResult.reason, {
        message: safeMessageText,
        sender: messageEntity.senderName,
        roomId: messageEntity.roomId,
        confidence: gatingResult.confidence,
        model: gatingResult.model
      });

      // Emit DECIDED_SILENT event (fire-and-forget — UI indicator)
      if (this.personaUser.client) {
        Events.emit<AIDecidedSilentEventData>(
        DataDaemon.jtagContext!,
        AI_DECISION_EVENTS.DECIDED_SILENT,
          {
            personaId: this.personaUser.id,
            personaName: this.personaUser.displayName,
            roomId: messageEntity.roomId,
            messageId: messageEntity.id,
            isHumanMessage: senderIsHuman,
            timestamp: Date.now(),
            confidence: gatingResult.confidence,
            reason: gatingResult.reason,
            gatingModel: gatingResult.model
          },
          {
            scope: EVENT_SCOPES.ROOM,
            scopeId: messageEntity.roomId,
          }
        ).catch(err => this.log(`⚠️ Event emit failed: ${err}`));
      }

      return;
    }

    // === RESPOND: LLM gating decided to respond, coordinate with other AIs ===

    // PHASE 5C: Prepare decision context for logging AFTER response generation
    // (We need the actual response content before we can log the complete decision)
    // After SILENT early-return above, TypeScript narrows gatingResult to GatingRespondResult.
    // filteredRagContext, ragContextSummary, confidence, reason, model are all guaranteed.
    const decisionContext = {
      actorId: this.personaUser.id,
      actorName: this.personaUser.displayName,
      actorType: 'ai-persona' as const,
      triggerEventId: messageEntity.id,
      ragContext: this.buildCoordinationRAGContext(gatingResult.filteredRagContext),
      visualContext: undefined,
      action: 'POSTED' as const,
      confidence: gatingResult.confidence,
      reasoning: gatingResult.reason,
      modelUsed: gatingResult.model,
      modelProvider: this.personaUser.modelConfig.provider ?? 'candle',
      sessionId: DataDaemon.jtagContext!.uuid,
      contextId: messageEntity.roomId,
      tags: [
        senderIsHuman ? 'human-sender' : 'ai-sender',
        isMentioned ? 'mentioned' : 'not-mentioned',
        'gating-respond'
      ]
    };

    this.personaUser.logAIDecision('RESPOND', gatingResult.reason, {
      message: safeMessageText,
      sender: messageEntity.senderName,
      roomId: messageEntity.roomId,
      mentioned: isMentioned,
      humanSender: senderIsHuman,
      confidence: gatingResult.confidence,
      model: gatingResult.model,
      ragContextSummary: gatingResult.ragContextSummary,
    });

    // Emit DECIDED_RESPOND event (fire-and-forget — UI indicator)
    if (this.personaUser.client) {
      Events.emit<AIDecidedRespondEventData>(
        DataDaemon.jtagContext!,
        AI_DECISION_EVENTS.DECIDED_RESPOND,
        {
          personaId: this.personaUser.id,
          personaName: this.personaUser.displayName,
          roomId: messageEntity.roomId,
          messageId: messageEntity.id,
          isHumanMessage: senderIsHuman,
          timestamp: Date.now(),
          confidence: gatingResult.confidence,
          reason: gatingResult.reason,
          gatingModel: gatingResult.model
        },
        {
          scope: EVENT_SCOPES.ROOM,
          scopeId: messageEntity.roomId,
        }
      ).catch(err => this.log(`⚠️ Event emit failed: ${err}`));
    }

    // === AUTONOMOUS DECISION: AI decides via RAG-based recipes ===
    // No centralized coordinator - each AI uses recipes to decide if they should contribute
    this.log(`✅ ${this.personaUser.displayName}: Autonomous decision to respond (RAG-based reasoning, conf=${gatingResult.confidence})`);

    // 🔧 POST-INFERENCE VALIDATION: Check if chat context changed during inference
    // Uses in-memory cache instead of SQLite query — O(1) instead of contended DB read
    const postInferenceStart = Date.now();
    const newMessages = PersonaMessageEvaluator.getRecentMessagesSince(
      messageEntity.roomId,
      new Date(messageEntity.timestamp)
    );

    if (newMessages.length > 0) {
      this.log(`🔄 ${this.personaUser.displayName}: Context changed during inference (${newMessages.length} new messages)`);

      // Check if other AIs already posted adequate responses
      // CRITICAL: Exclude the original trigger message AND the sending persona
      const otherAIResponses = newMessages.filter(m =>
        m.id !== messageEntity.id &&  // Exclude the original trigger message
        m.senderType !== 'human' &&
        m.senderId !== this.personaUser.id &&
        m.senderId !== messageEntity.senderId  // Exclude original sender's other messages
      );

      if (otherAIResponses.length > 0) {
        // Check if any response is adequate (substantial and related)
        const adequacyResult = await this.checkResponseAdequacy(
          messageEntity,
          otherAIResponses  // Already flat ChatMessageEntity objects from cache
        );

        if (adequacyResult.isAdequate) {
          this.log(`⏭️ ${this.personaUser.displayName}: Post-inference skip - adequate AI response exists`);
          this.log(`   Skipped because: ${adequacyResult.reason}`);

          // Emit DECIDED_SILENT event (fire-and-forget — UI indicator)
          if (this.personaUser.client) {
            Events.emit<AIDecidedSilentEventData>(
              DataDaemon.jtagContext!,
              AI_DECISION_EVENTS.DECIDED_SILENT,
              {
                personaId: this.personaUser.id,
                personaName: this.personaUser.displayName,
                roomId: messageEntity.roomId,
                messageId: messageEntity.id,
                isHumanMessage: senderIsHuman,
                timestamp: Date.now(),
                reason: `Post-inference re-evaluation: ${adequacyResult.reason}`,
                confidence: adequacyResult.confidence,
                gatingModel: 'post-inference-heuristic'
              },
              {
                scope: EVENT_SCOPES.ROOM,
                scopeId: messageEntity.roomId
              }
            ).catch(err => this.log(`⚠️ Event emit failed: ${err}`));
          }

          this.personaUser.logAIDecision('SILENT', `Post-inference skip: ${adequacyResult.reason}`, {
            message: messageEntity.content.text,
            sender: messageEntity.senderName,
            roomId: messageEntity.roomId
          });

          return; // Exit early - don't generate response
        }
      }

      this.log(`   New messages: ${newMessages.map(m => `[${m.senderName}] ${contentPreview(m.content, 50)}`).join(', ')}`);
    }

    this.log(`⏱️ ${this.personaUser.displayName}: [INNER] post-inference validation=${Date.now() - postInferenceStart}ms`);

    // 🔧 PHASE: Update RAG context (fire-and-forget — bookkeeping, not needed before generation)
    // The pre-built RAG context from evaluateShouldRespond already has current messages.
    // This just appends the trigger message to the stored context entity for next cycle.
    this.personaUser.memory.updateRAGContext(messageEntity.roomId, messageEntity)
      .catch(err => this.log(`⚠️ RAG context update failed: ${err}`));
    this.log(`🔧 ${this.personaUser.displayName}: [PHASE 1/3] RAG context update dispatched (fire-and-forget)`);

    // 🔧 PHASE: Emit GENERATING event (fire-and-forget — UI indicator)
    this.log(`🔧 ${this.personaUser.displayName}: [PHASE 2/3] Emitting GENERATING event...`);
    if (this.personaUser.client) {
      Events.emit<AIGeneratingEventData>(
        DataDaemon.jtagContext!,
        AI_DECISION_EVENTS.GENERATING,
        {
          personaId: this.personaUser.id,
          personaName: this.personaUser.displayName,
          roomId: messageEntity.roomId,
          messageId: messageEntity.id,
          isHumanMessage: senderIsHuman,
          timestamp: Date.now(),
          responseModel: this.personaUser.entity?.personaConfig?.responseModel ?? 'default'
        },
        {
          scope: EVENT_SCOPES.ROOM,
          scopeId: messageEntity.roomId
        }
      ).catch(err => this.log(`⚠️ Event emit failed: ${err}`));
    }
    this.log(`✅ ${this.personaUser.displayName}: [PHASE 2/3] GENERATING event emitted`);

    // 🔧 PHASE: Generate and post response
    this.log(`🔧 TRACE-POINT-B: Before respondToMessage call (timestamp=${Date.now()})`);
    this.log(`🔧 ${this.personaUser.displayName}: [PHASE 3/3] Calling respondToMessage...`);
    await this.personaUser.respondToMessage(messageEntity, decisionContext, gatingResult.filteredRagContext);
    this.log(`🔧 TRACE-POINT-C: After respondToMessage returned (timestamp=${Date.now()})`);
    this.log(`✅ ${this.personaUser.displayName}: [PHASE 3/3] Response posted successfully`);

    // Signal conversation activity (warms room — active conversation stays alive)
    getChatCoordinator().onMessageServiced(messageEntity.roomId, this.personaUser.id);

    // Track response for rate limiting (Rust is sole authority)
    this.personaUser.rustCognition.trackResponse(messageEntity.roomId)
      .catch(err => this.log(`⚠️ Rust trackResponse failed (non-fatal): ${err}`));

    // PHASE 2: Track activity in PersonaState (energy depletion, mood calculation)
    // Recalculate priority to estimate complexity (higher priority = more engaging conversation)
    const messageComplexity = calculateMessagePriority(
      {
        content: messageEntity.content?.text || '',
        timestamp: this.personaUser.timestampToNumber(messageEntity.timestamp),
        roomId: messageEntity.roomId
      },
      {
        displayName: this.personaUser.displayName,
        id: this.personaUser.id,
        recentRooms: Array.from(this.personaUser.myRoomIds) // Convert Set<string> to UUID[]
      }
    );
    // Estimate duration based on average AI response time
    const estimatedDurationMs = 3000; // Average AI response time (3 seconds)
    await this.personaUser.personaState.recordActivity(estimatedDurationMs, messageComplexity);

    this.log(`🧠 ${this.personaUser.displayName}: State updated (energy=${this.personaUser.personaState.getState().energy.toFixed(2)}, mood=${this.personaUser.personaState.getState().mood})`);
  }

  /**
   * Build CoordinationDecision RAGContext from ChatRAGBuilder output
   * Converts domain-specific RAG format to universal decision logging format
   */
  private buildCoordinationRAGContext(filteredRagContext: PipelineRAGContext): RAGContext {
    return {
      identity: {
        systemPrompt: filteredRagContext.identity.systemPrompt,
        bio: this.personaUser.entity?.bio ?? '',
        role: this.personaUser.displayName
      },
      conversationHistory: filteredRagContext.conversationHistory.map(msg => ({
        role: msg.role,
        content: msg.content,
        timestamp: msg.timestamp ?? Date.now()
      })),
      artifacts: (filteredRagContext.artifacts ?? []).map(a => ({
        type: this.mapArtifactType(a.type),
        name: a.url ?? a.type,
        content: a.content ?? a.base64 ?? '',
        mimeType: undefined,
      })),
      privateMemories: (filteredRagContext.privateMemories ?? []).map(m => ({
        type: m.type,
        content: m.content,
        relevance: m.relevanceScore,
      })),
      metadata: {
        timestamp: Date.now(),
        tokenCount: filteredRagContext.metadata.messageCount,
        contextWindow: 4096
      }
    };
  }

  /** Map pipeline artifact types to coordination logging's narrower type union. */
  private mapArtifactType(pipelineType: RAGArtifact['type']): 'image' | 'file' | 'code' {
    switch (pipelineType) {
      case 'image':
      case 'screenshot':
      case 'video':
      case 'audio':
        return 'image';
      case 'data':
      case 'benchmark':
        return 'code';
      case 'file':
        return 'file';
    }
  }

  /**
   * Check if existing AI responses are adequate (no need for another response).
   *
   * ONE Rust IPC call checks all responses in batch — replaces N individual
   * textSimilarity calls. Rust handles length filtering (>100 chars) and
   * Jaccard n-gram similarity (>0.2 threshold) internally.
   */
  private async checkResponseAdequacy(
    originalMessage: ProcessableMessage,
    otherResponses: ChatMessageEntity[]
  ): Promise<{ isAdequate: boolean; confidence: number; reason: string }> {
    const originalText = originalMessage.content?.text || '';

    // Build response array for Rust — single IPC call handles all comparisons
    const responses = otherResponses.map(r => ({
      sender_name: r.senderName ?? 'Unknown',
      text: r.content?.text || '',
    }));

    const result = await this.personaUser.rustCognition.checkAdequacy(originalText, responses);

    return {
      isAdequate: result.is_adequate,
      confidence: result.confidence,
      reason: result.reason,
    };
  }

  /**
   * Evaluate whether to respond using Decision Adapter Chain
   *
   * PHASE 6: Refactored to use adapter pattern (fast-path, thermal, LLM)
   * Instead of hardcoded logic, delegates to chain of decision adapters.
   */
  async evaluateShouldRespond(
    message: ProcessableMessage,
    senderIsHuman: boolean,
    isMentioned: boolean,
    preComputedDecision?: FastPathDecision
  ): Promise<GatingResult> {
    const startTime = Date.now();

    try {
      // RUST COGNITION: Fast-path decision
      // If pre-computed from serviceCycleFull, skip the separate IPC call entirely
      let rustDecision: { should_respond: boolean; confidence: number; reason: string; decision_time_ms: number; fast_path_used: boolean };

      if (preComputedDecision) {
        // Decision already computed by Rust in serviceCycleFull (saves one IPC round-trip)
        rustDecision = preComputedDecision;
        this.log(`🦀 ${this.personaUser.displayName}: Using pre-computed decision (saved IPC call): ${rustDecision.should_respond ? 'RESPOND' : 'SILENT'} (${rustDecision.decision_time_ms.toFixed(2)}ms, fast_path=${rustDecision.fast_path_used})`);
      } else {
        // Fallback: make separate IPC call (for code paths that don't go through CNS)
        const senderType: SenderType = senderIsHuman ? 'human' : 'persona';
        const priority = calculateMessagePriority(
          {
            content: message.content?.text ?? '',
            timestamp: this.personaUser.timestampToNumber(message.timestamp),
            roomId: message.roomId
          },
          {
            displayName: this.personaUser.displayName,
            id: this.personaUser.id
          }
        );

        const inboxRequest = toInboxMessageRequest(
          {
            id: message.id,
            roomId: message.roomId,
            senderId: message.senderId,
            senderName: message.senderName,
            content: message.content?.text ?? '',
            timestamp: this.personaUser.timestampToNumber(message.timestamp)
          },
          senderType,
          priority,
          'chat'
        );

        const ipcStart = performance.now();
        rustDecision = await this.personaUser.rustCognition.fastPathDecision(inboxRequest);
        const ipcMs = performance.now() - ipcStart;

        this.log(`🦀 ${this.personaUser.displayName}: Rust decision (separate IPC, ${ipcMs.toFixed(1)}ms): ${rustDecision.should_respond ? 'RESPOND' : 'SILENT'} (${rustDecision.decision_time_ms.toFixed(2)}ms, fast_path=${rustDecision.fast_path_used})`);
      }

      // OPTIMIZATION: Only build RAG context if we're going to respond.
      // Rust fast-path already decided should_respond — for SILENT decisions,
      // skip the 40-240ms RAG build entirely.
      if (!rustDecision.should_respond) {
        const totalMs = Date.now() - startTime;
        this.log(`[TIMING] ${this.personaUser.displayName}: evaluateShouldRespond total=${totalMs}ms (rag=SKIPPED/silent, preComputed=${!!preComputedDecision})`);

        return {
          shouldRespond: false as const,
          confidence: rustDecision.confidence,
          reason: rustDecision.reason,
          model: rustDecision.fast_path_used ? 'RustFastPath' : 'RustCognition',
        };
      }

      // RESPOND path: Build FULL RAG context (with memories + artifacts).
      // This context will be passed through to PersonaResponseGenerator,
      // eliminating the redundant second RAG build that previously happened there.
      const ragStart = performance.now();
      const ragBuilder = new ChatRAGBuilder(this.log.bind(this));
      const provider = this.personaUser.modelConfig.provider || 'candle';
      const ragContext = await ragBuilder.buildContext(
        message.roomId,
        this.personaUser.id,
        {
          modelId: this.personaUser.modelConfig.model,
          maxMemories: 5,           // Full context: include memories for LLM prompt
          includeArtifacts: true,    // Full context: include vision artifacts
          includeMemories: true,     // Full context: include Hippocampus LTM
          excludeMessageIds: this.personaUser.taskTracker.getProcessedToolResults(),
          provider,
          toolCapability: getToolCapability(provider, this.personaUser.modelConfig),
          currentMessage: {
            role: 'user',
            content: message.content.text,
            name: message.senderName,
            timestamp: this.personaUser.timestampToNumber(message.timestamp)
          }
        }
      );
      const ragMs = performance.now() - ragStart;
      const totalMs = Date.now() - startTime;

      this.log(`[TIMING] ${this.personaUser.displayName}: evaluateShouldRespond total=${totalMs}ms (rag=${ragMs.toFixed(1)}ms/full, preComputed=${!!preComputedDecision})`);

      return {
        shouldRespond: true as const,
        confidence: rustDecision.confidence,
        reason: rustDecision.reason,
        model: rustDecision.fast_path_used ? 'RustFastPath' : 'RustCognition',
        filteredRagContext: ragContext,
        ragContextSummary: {
          totalMessages: ragContext.conversationHistory.length,
          filteredMessages: ragContext.conversationHistory.length,
          timeWindowMinutes: 30
        }
      };

    } catch (error: any) {
      this.log(`❌ ${this.personaUser.displayName}: Should-respond evaluation failed:`, error);

      const durationMs = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);

      // Emit cognition event for error case (fire-and-forget — telemetry)
      Events.emit<StageCompleteEvent>(
        DataDaemon.jtagContext!,
        COGNITION_EVENTS.STAGE_COMPLETE,
        {
          messageId: message.id,
          personaId: this.personaUser.id,
          contextId: message.roomId,
          stage: 'should-respond',
          metrics: {
            stage: 'should-respond',
            durationMs,
            resourceUsed: 0,
            maxResource: 100,
            percentCapacity: 0,
            percentSpeed: calculateSpeedScore(durationMs, 'should-respond'),
            status: 'bottleneck',
            metadata: {
              error: true,
              errorMessage
            }
          },
          timestamp: Date.now()
        }
      ).catch(err => this.log(`⚠️ Stage event emit failed: ${err}`));

      // Emit ERROR event to update UI status (clears "thinking" status)
      if (this.personaUser.client) {
        Events.emit<AIErrorEventData>(
          DataDaemon.jtagContext!,
          AI_DECISION_EVENTS.ERROR,
          {
            personaId: this.personaUser.id,
            personaName: this.personaUser.displayName,
            roomId: message.roomId,
            messageId: message.id,
            isHumanMessage: message.senderType === 'human',
            timestamp: Date.now(),
            error: errorMessage,
            phase: 'evaluating'
          },
          {
            scope: EVENT_SCOPES.ROOM,
            scopeId: message.roomId
          }
        ).catch(err => this.log(`⚠️ Error event emit failed: ${err}`));
      }

      // Error in evaluation = SILENT. No fallback guessing.
      return {
        shouldRespond: false as const,
        confidence: 0,
        reason: `Error in evaluation: ${errorMessage}`,
        model: 'error'
      };
    }
  }
}
