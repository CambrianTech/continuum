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
import { inspect } from 'util';
import { Events } from '../../../core/shared/Events';
import { COLLECTIONS } from '../../../shared/Constants';
import type { ChatMessageEntity } from '../../../data/entities/ChatMessageEntity';
import type { ProcessableMessage } from './QueueItemTypes';
import type { UserEntity } from '../../../data/entities/UserEntity';
import type { RoomEntity } from '../../../data/entities/RoomEntity';
import { CognitionLogger } from './cognition/CognitionLogger';
import { SignalDetector, getSignalDetector } from './SignalDetector';
import { getTrainingBuffer } from './TrainingBuffer';
import type { Task } from './cognition/reasoning/types';
import { ChatRAGBuilder } from '../../../rag/builders/ChatRAGBuilder';
import { CoordinationDecisionLogger, type LogDecisionParams } from '../../../coordination/server/CoordinationDecisionLogger';
import type { RAGContext } from '../../../data/entities/CoordinationDecisionEntity';
import type { RAGContext as PipelineRAGContext, RAGArtifact } from '../../../rag/shared/RAGTypes';
import type { AIDecisionContext } from '../../../ai/server/AIDecisionService';
import { AIDecisionService } from '../../../ai/server/AIDecisionService';
import { contentPreview, truncate } from '../../../../shared/utils/StringUtils';
import type { DecisionContext } from './cognition/adapters/IDecisionAdapter';
import { getChatCoordinator } from '../../../coordination/server/ChatCoordinationStream';
import { calculateMessagePriority } from './PersonaInbox';
import { toInboxMessageRequest } from './RustCognitionBridge';
import type { SenderType } from '../../../../shared/generated';
import type { FastPathDecision } from './central-nervous-system/CNSTypes';
import { personaSleepManager } from '@commands/ai/sleep/server/AiSleepServerCommand';
import {
  AI_DECISION_EVENTS,
  type AIEvaluatingEventData,
  type AIDecidedSilentEventData,
  type AIDecidedRespondEventData,
  type AIGeneratingEventData
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

  constructor(private readonly personaUser: PersonaUser) {
    this.signalDetector = getSignalDetector();
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
      const result = await DataDaemon.query<ChatMessageEntity>({
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
      const result = await DataDaemon.query<ChatMessageEntity>({
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

    // SIGNAL DETECTION: Analyze message content for training signals
    // Fire-and-forget - AI classifier determines if content is feedback
    this.detectAndBufferTrainingSignal(messageEntity).catch(err => {
      this.log(`⚠️ ${this.personaUser.displayName}: Signal detection failed (non-fatal):`, err);
    });

    // STEP 1: Create Task from message
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

    this.log(`🧠 ${this.personaUser.displayName}: COGNITION - Created task for message from ${messageEntity.senderName}`);

    // STEP 2: Generate Plan
    const plan = await this.personaUser.planFormulator.formulatePlan(task);
    this.log(`📋 ${this.personaUser.displayName}: COGNITION - Plan: ${plan.goal}`);
    this.log(`   Steps: ${plan.steps.map((s: any) => s.action).join(' → ')}`);

    // LOG: Plan formulation
    await CognitionLogger.logPlanFormulation(
      this.personaUser.id,
      this.personaUser.displayName,
      task,
      plan,
      'chat',
      messageEntity.roomId,
      'template-based'  // SimplePlanFormulator uses templates
    );

    // STEP 3: Update SelfState - set focus
    await this.personaUser.selfState.updateFocus({
      activity: 'chat-response',
      objective: plan.goal,
      intensity: task.priority
    });
    await this.personaUser.selfState.updateLoad(0.2); // Chat response adds cognitive load

    // LOG: State snapshot after focus/load update
    const selfState = await this.personaUser.selfState.get();
    const workingMemoryEntries = await this.personaUser.workingMemory.recall({
      domain: 'chat',
      contextId: messageEntity.roomId,
      limit: 100
    });
    const capacity = await this.personaUser.workingMemory.getCapacity('chat');

    await CognitionLogger.logStateSnapshot(
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

    // STEP 4: Store initial observation in WorkingMemory
    await this.personaUser.workingMemory.store({
      domain: 'chat',
      contextId: messageEntity.roomId,
      thoughtType: 'observation',
      thoughtContent: `Received message from ${messageEntity.senderName}: "${safeMessageText.slice(0, 200)}"`,
      importance: task.priority,
      shareable: false
    });

    // STEP 5: Execute plan steps (existing chat logic inside)
    try {
      // Mark step 1 complete: "Recall relevant context"
      plan.steps[0].completed = true;
      plan.steps[0].completedAt = Date.now();

      // Execute step 2: "Generate thoughtful response" (existing logic)
      await this.evaluateAndPossiblyRespond(messageEntity, senderIsHuman, safeMessageText, preComputedDecision);

      // If we got here, response was generated (or decision was SILENT)
      plan.steps[1].completed = true;
      plan.steps[1].completedAt = Date.now();

      // Note: Step 3 "Post message" happens inside evaluateAndPossiblyRespond if decision was RESPOND
      if (plan.steps.length > 2) {
        plan.steps[2].completed = true;
        plan.steps[2].completedAt = Date.now();
      }

      // STEP 6: Store outcome in WorkingMemory
      await this.personaUser.workingMemory.store({
        domain: 'chat',
        contextId: messageEntity.roomId,
        thoughtType: 'reflection',
        thoughtContent: `Completed response plan for message from ${messageEntity.senderName}`,
        importance: 0.5,
        shareable: false
      });

      this.log(`✅ ${this.personaUser.displayName}: COGNITION - Plan completed successfully`);

      // LOG: Plan completion
      await CognitionLogger.logPlanCompletion(
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

      // LOG: Plan failure
      await CognitionLogger.logPlanCompletion(
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
      await this.personaUser.selfState.clearFocus();
      await this.personaUser.selfState.updateLoad(-0.2); // Remove the load we added

      const duration = Date.now() - taskStartTime;
      this.log(`🧠 ${this.personaUser.displayName}: COGNITION - Task complete (${duration}ms)`);
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
    // STEP 2: Check response cap (prevent infinite loops)
    if (this.personaUser.rateLimiter.hasReachedResponseCap(messageEntity.roomId)) {
      const currentCount = this.personaUser.rateLimiter.getResponseCount(messageEntity.roomId);
      const config = this.personaUser.rateLimiter.getConfig();
      this.personaUser.logAIDecision('SILENT', `Response cap reached (${currentCount}/${config.maxResponsesPerSession})`, {
        message: safeMessageText,
        sender: messageEntity.senderName,
        roomId: messageEntity.roomId
      });
      return;
    }

    // STEP 3: Check if mentioned
    const isMentioned = this.isPersonaMentioned(safeMessageText);

    // STEP 4: Check rate limiting (before expensive LLM call)
    if (this.personaUser.rateLimiter.isRateLimited(messageEntity.roomId)) {
      const info = this.personaUser.rateLimiter.getRateLimitInfo(messageEntity.roomId);
      this.personaUser.logAIDecision('SILENT', `Rate limited, wait ${info.waitTimeSeconds?.toFixed(1)}s more`, {
        message: safeMessageText,
        sender: messageEntity.senderName,
        roomId: messageEntity.roomId
      });
      return;
    }

    // STEP 5: Check voluntary sleep mode (before expensive LLM call)
    // AIs can put themselves to sleep to manage attention autonomously
    const sleepMode = personaSleepManager.getMode(this.personaUser.id);
    if (sleepMode !== 'active') {
      // Detect if this is a new topic (enables until_topic sleep mode)
      const isNewTopic = await this.detectNewTopic(safeMessageText, messageEntity.roomId);

      const shouldRespondInSleepMode = personaSleepManager.shouldRespond(this.personaUser.id, {
        isHuman: senderIsHuman,
        isMention: isMentioned,
        isNewTopic
      });

      if (!shouldRespondInSleepMode) {
        this.log(`😴 ${this.personaUser.displayName}: In ${sleepMode} mode, skipping message from ${messageEntity.senderName}`);
        this.personaUser.logAIDecision('SILENT', `Voluntary sleep mode: ${sleepMode} (isHuman=${senderIsHuman}, isMention=${isMentioned})`, {
          message: safeMessageText,
          sender: messageEntity.senderName,
          roomId: messageEntity.roomId,
          humanSender: senderIsHuman,
          mentioned: isMentioned
        });
        return;
      }

      this.log(`😴 ${this.personaUser.displayName}: In ${sleepMode} mode but responding (isHuman=${senderIsHuman}, isMention=${isMentioned})`);
    }

    // === EVALUATE: Use LLM-based intelligent gating to decide if should respond ===
    // Emit EVALUATING event for real-time feedback
    if (this.personaUser.client) {
      await Events.emit<AIEvaluatingEventData>(
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
      );
    }

    const gatingResult = await this.evaluateShouldRespond(messageEntity, senderIsHuman, isMentioned, preComputedDecision);

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

      // Emit DECIDED_SILENT event
      if (this.personaUser.client) {
        await Events.emit<AIDecidedSilentEventData>(
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
        );
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

    // Emit DECIDED_RESPOND event
    if (this.personaUser.client) {
      await Events.emit<AIDecidedRespondEventData>(
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
      );
    }

    // === AUTONOMOUS DECISION: AI decides via RAG-based recipes ===
    // No centralized coordinator - each AI uses recipes to decide if they should contribute
    this.log(`✅ ${this.personaUser.displayName}: Autonomous decision to respond (RAG-based reasoning, conf=${gatingResult.confidence})`);
    this.log(`🔧 TRACE-POINT-A: About to check for new messages (timestamp=${Date.now()})`);

    // 🔧 POST-INFERENCE VALIDATION: Check if chat context changed during inference
    // During the 3-5 seconds of inference, other AIs may have already posted responses
    // Give this AI a chance to see those new responses and reject its own if redundant
    const newMessagesQuery = await DataDaemon.query<ChatMessageEntity>({
      collection: COLLECTIONS.CHAT_MESSAGES,
      filter: {
        roomId: messageEntity.roomId,
        timestamp: { $gt: messageEntity.timestamp }  // Messages newer than the trigger
      },
      limit: 10
    });

    const newMessages = newMessagesQuery.data || [];
    if (newMessages.length > 0) {
      this.log(`🔄 ${this.personaUser.displayName}: Context changed during inference (${newMessages.length} new messages)`);

      // Check if other AIs already posted adequate responses
      // CRITICAL: Exclude the original trigger message AND the sending persona
      // Bug fix: Original message was slipping through due to timestamp precision,
      // causing 100% self-similarity match and blocking all AI responses
      const otherAIResponses = newMessages.filter(m =>
        m.id !== messageEntity.id &&  // Exclude the original trigger message
        m.data.senderType !== 'human' &&
        m.data.senderId !== this.personaUser.id &&
        m.data.senderId !== messageEntity.senderId  // Exclude original sender's other messages
      );

      if (otherAIResponses.length > 0) {
        // Check if any response is adequate (substantial and related)
        const adequacyResult = this.checkResponseAdequacy(
          messageEntity,
          otherAIResponses.map(r => r.data)
        );

        if (adequacyResult.isAdequate) {
          this.log(`⏭️ ${this.personaUser.displayName}: Post-inference skip - adequate AI response exists`);
          this.log(`   Skipped because: ${adequacyResult.reason}`);

          // Emit DECIDED_SILENT event
          if (this.personaUser.client) {
            await Events.emit<AIDecidedSilentEventData>(
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
            );
          }

          this.personaUser.logAIDecision('SILENT', `Post-inference skip: ${adequacyResult.reason}`, {
            message: messageEntity.content.text,
            sender: messageEntity.senderName,
            roomId: messageEntity.roomId
          });

          return; // Exit early - don't generate response
        }
      }

      this.log(`   New messages: ${newMessages.map(m => `[${m.data.senderName}] ${contentPreview(m.data.content, 50)}`).join(', ')}`);
    }

    // 🔧 PHASE: Update RAG context
    this.log(`🔧 ${this.personaUser.displayName}: [PHASE 1/3] Updating RAG context...`);
    await this.personaUser.memory.updateRAGContext(messageEntity.roomId, messageEntity);
    this.log(`✅ ${this.personaUser.displayName}: [PHASE 1/3] RAG context updated`);

    // 🔧 PHASE: Emit GENERATING event (using auto-context via sharedInstance)
    this.log(`🔧 ${this.personaUser.displayName}: [PHASE 2/3] Emitting GENERATING event...`);
    if (this.personaUser.client) {
      await Events.emit<AIGeneratingEventData>(
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
      );
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

    // Track response for rate limiting
    this.personaUser.rateLimiter.trackResponse(messageEntity.roomId);

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
   * Check if this persona is mentioned in a message
   * Supports @username mentions and channel directives
   *
   * TODO Phase 2: Use dedicated mention/directive events instead of text parsing
   */
  private isPersonaMentioned(safeMessageText: string): boolean {
    const safeMessageTextLower = safeMessageText.toLowerCase();
    const displayNameLower = this.personaUser.displayName.toLowerCase();
    const uniqueIdLower = this.personaUser.entity.uniqueId?.toLowerCase() || '';

    // Check for @mentions ANYWHERE in message: "@PersonaName" or "@uniqueid"
    // Works like Discord/Slack - @ can be at start, middle, or end
    if (safeMessageTextLower.includes(`@${displayNameLower}`) ||
        safeMessageTextLower.includes(`@${uniqueIdLower}`)) {
      return true;
    }

    // Check for direct address at START: "PersonaName," or "PersonaName:"
    // e.g. "Teacher AI, explain closures" or "teacher-ai: what's up"
    if (safeMessageTextLower.startsWith(displayNameLower + ',') ||
        safeMessageTextLower.startsWith(displayNameLower + ':') ||
        safeMessageTextLower.startsWith(uniqueIdLower + ',') ||
        safeMessageTextLower.startsWith(uniqueIdLower + ':')) {
      return true;
    }

    return false;
  }


  /**
   * Get domain keywords for this persona
   * Reads from UserEntity.personaConfig if available, otherwise infers from name
   */
  private getPersonaDomainKeywords(): string[] {
    // Read from entity configuration if available
    if (this.personaUser.entity?.personaConfig?.domainKeywords?.length) {
      return [...this.personaUser.entity.personaConfig.domainKeywords];
    }

    // Fallback: infer from persona name (temporary until all personas configured)
    const nameLower = this.personaUser.displayName.toLowerCase();

    if (nameLower.includes('teacher') || nameLower.includes('academy')) {
      return ['teaching', 'education', 'learning', 'explain', 'understand', 'lesson'];
    }
    if (nameLower.includes('code') || nameLower.includes('dev') || nameLower.includes('review')) {
      return ['code', 'programming', 'function', 'bug', 'typescript', 'javascript'];
    }
    if (nameLower.includes('plan') || nameLower.includes('architect')) {
      return ['plan', 'architecture', 'design', 'structure', 'organize'];
    }

    // Default: general AI assistant keywords
    return ['help', 'question', 'what', 'how', 'why', 'explain'];
  }

  /**
   * Calculate heuristics for response decision (Phase 2)
   * NO API calls - pure logic based on conversation history
   */
  private async calculateResponseHeuristics(messageEntity: ProcessableMessage): Promise<{
    containsQuestion: boolean;
    conversationTemp: 'HOT' | 'WARM' | 'COOL' | 'COLD';
    myParticipationRatio: number;
    secondsSinceMyLastMessage: number;
    appearsToBeMyTurn: boolean;
  }> {
    // 1. Question detection (simple)
    const containsQuestion = messageEntity.content?.text?.includes('?') || false;

    // 2. Get recent messages for context
    const recentMessages = await DataDaemon.query<ChatMessageEntity>({
      collection: COLLECTIONS.CHAT_MESSAGES,
      filter: { roomId: messageEntity.roomId },
      sort: [{ field: 'timestamp', direction: 'desc' }],
      limit: 10
    });

    const messages: ChatMessageEntity[] = recentMessages.success && recentMessages.data
      ? recentMessages.data.map(record => record.data)
      : [];

    // 3. Calculate conversation temperature (time between recent messages)
    let conversationTemp: 'HOT' | 'WARM' | 'COOL' | 'COLD' = 'COLD';
    if (messages.length >= 2) {
      const timeDiffs: number[] = [];
      for (let i = 0; i < messages.length - 1; i++) {
        const t1 = new Date(messages[i].timestamp).getTime();
        const t2 = new Date(messages[i + 1].timestamp).getTime();
        const diff = t1 - t2;
        timeDiffs.push(diff / 1000); // Convert to seconds
      }
      const avgTimeBetween = timeDiffs.reduce((a, b) => a + b, 0) / timeDiffs.length;

      if (avgTimeBetween < 10) conversationTemp = 'HOT';      // <10s between messages
      else if (avgTimeBetween < 30) conversationTemp = 'WARM'; // <30s
      else if (avgTimeBetween < 60) conversationTemp = 'COOL'; // <60s
      else conversationTemp = 'COLD';                           // >60s
    }

    // 4. Calculate my participation ratio
    const myMessages = messages.filter(m => m.senderId === this.personaUser.id);
    const myParticipationRatio = messages.length > 0 ? myMessages.length / messages.length : 0;

    // 5. Time since my last message
    const myLastMessage = myMessages[0];
    const secondsSinceMyLastMessage = myLastMessage
      ? (Date.now() - new Date(myLastMessage.timestamp).getTime()) / 1000
      : 999;

    // 6. Turn-taking pattern - is it my turn?
    // My turn if: last message wasn't mine AND I haven't spoken recently
    const lastMessage = messages[0];
    const appearsToBeMyTurn =
      lastMessage?.senderId !== this.personaUser.id &&
      secondsSinceMyLastMessage > 30;

    return {
      containsQuestion,
      conversationTemp,
      myParticipationRatio,
      secondsSinceMyLastMessage,
      appearsToBeMyTurn
    };
  }

  /**
   * Check if a sender is a human user (not AI/persona/agent)
   * CRITICAL for preventing infinite response loops between AI users
   */
  private async isSenderHuman(senderId: UUID): Promise<boolean> {
    if (!this.personaUser.client) {
      this.log(`⚠️  PersonaUser ${this.personaUser.displayName}: Cannot check sender type - no client, BLOCKING response`);
      return false; // Fail CLOSED - don't respond if we can't verify (prevents startup loops)
    }

    try {
      // Query the sender's UserEntity to check their type using DataDaemon directly
      const sender = await DataDaemon.read<UserEntity>(COLLECTIONS.USERS, senderId);

      if (!sender) {
        this.log(`⚠️  PersonaUser ${this.personaUser.displayName}: Could not read sender ${senderId}, BLOCKING response`);
        return false; // Fail CLOSED - don't respond if database fails (prevents loops)
      }

      return sender.type === 'human';

    } catch (error: any) {
      this.log(`❌ PersonaUser ${this.personaUser.displayName}: Error checking sender type, BLOCKING response:`, error);
      return false; // Fail CLOSED on error (prevents loops)
    }
  }

  /**
   * Detect if current message is a new topic vs continuation of existing conversation
   * Uses fast n-gram text similarity (no embeddings needed)
   *
   * @param currentText - The incoming message text
   * @param roomId - Room to query recent messages from
   * @param threshold - Similarity threshold (below = new topic). Default 0.3
   * @returns True if this appears to be a new topic
   */
  private async detectNewTopic(
    currentText: string,
    roomId: UUID,
    threshold: number = 0.3
  ): Promise<boolean> {
    // Query recent messages from this room
    const recentMessages = await DataDaemon.query<ChatMessageEntity>({
      collection: COLLECTIONS.CHAT_MESSAGES,
      filter: { roomId },
      sort: [{ field: 'timestamp', direction: 'desc' }],
      limit: 5
    });

    const messages = recentMessages.data || [];
    if (messages.length === 0) {
      return true; // No history = definitely new topic
    }

    // Combine recent message texts
    const recentTexts = messages
      .map(m => m.data.content?.text || '')
      .filter(t => t.length > 0)
      .join(' ');

    if (recentTexts.length === 0) {
      return true; // No text content = new topic
    }

    // Fast text similarity using n-gram Jaccard
    const similarity = this.computeTextSimilarity(currentText, recentTexts);

    // Below threshold = new topic
    const isNewTopic = similarity < threshold;

    if (isNewTopic) {
      this.log(`🔄 ${this.personaUser.displayName}: New topic detected (similarity=${similarity.toFixed(2)} < ${threshold})`);
    }

    return isNewTopic;
  }

  /**
   * Compute text similarity using n-gram Jaccard coefficient
   * Fast O(n) algorithm - no embeddings or API calls needed
   *
   * Uses unigrams + bigrams for better phrase detection
   */
  private computeTextSimilarity(text1: string, text2: string): number {
    // Tokenize into words (filter short words as noise)
    const tokenize = (text: string): string[] => {
      return text
        .toLowerCase()
        .split(/\W+/)
        .filter(word => word.length > 2);
    };

    const tokens1 = tokenize(text1);
    const tokens2 = tokenize(text2);

    if (tokens1.length === 0 || tokens2.length === 0) {
      return 0;
    }

    // Generate unigrams + bigrams for better phrase matching
    const generateNgrams = (tokens: string[]): Set<string> => {
      const ngrams = new Set<string>();

      // Unigrams
      tokens.forEach(t => ngrams.add(t));

      // Bigrams
      for (let i = 0; i < tokens.length - 1; i++) {
        ngrams.add(`${tokens[i]}_${tokens[i + 1]}`);
      }

      return ngrams;
    };

    const ngrams1 = generateNgrams(tokens1);
    const ngrams2 = generateNgrams(tokens2);

    // Jaccard coefficient = |intersection| / |union|
    const intersection = [...ngrams1].filter(n => ngrams2.has(n)).length;
    const union = new Set([...ngrams1, ...ngrams2]).size;

    return union > 0 ? intersection / union : 0;
  }

  /**
   * Check if existing AI responses are adequate (no need for another response)
   *
   * Used for post-inference re-evaluation to prevent redundant responses
   * when another AI already answered during our inference time.
   */
  private checkResponseAdequacy(
    originalMessage: ProcessableMessage,
    otherResponses: ChatMessageEntity[]
  ): { isAdequate: boolean; confidence: number; reason: string } {
    const originalText = originalMessage.content?.text || '';

    for (const response of otherResponses) {
      const responseText = response.content?.text || '';

      // Skip short responses (likely not adequate)
      if (responseText.length < 100) continue;

      // Check if response is related to original question
      const similarity = this.computeTextSimilarity(originalText, responseText);

      // Substantial response (>100 chars) that's related to the question (>0.2 similarity)
      if (similarity > 0.2) {
        return {
          isAdequate: true,
          confidence: Math.min(similarity + 0.5, 1.0), // Boost confidence for related responses
          reason: `${response.senderName} already provided a substantial response (${responseText.length} chars, ${(similarity * 100).toFixed(0)}% related)`
        };
      }
    }

    return {
      isAdequate: false,
      confidence: 0,
      reason: 'No adequate responses found'
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
      const ragContext = await ragBuilder.buildContext(
        message.roomId,
        this.personaUser.id,
        {
          modelId: this.personaUser.modelConfig.model,
          maxMemories: 5,           // Full context: include memories for LLM prompt
          includeArtifacts: true,    // Full context: include vision artifacts
          includeMemories: true,     // Full context: include Hippocampus LTM
          excludeMessageIds: this.personaUser.taskTracker.getProcessedToolResults(),
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

      // Emit cognition event for error case
      await Events.emit<StageCompleteEvent>(
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
              errorMessage: error instanceof Error ? error.message : String(error)
            }
          },
          timestamp: Date.now()
        }
      );

      // Error in evaluation = SILENT. No fallback guessing.
      return {
        shouldRespond: false as const,
        confidence: 0,
        reason: `Error in evaluation: ${error instanceof Error ? error.message : String(error)}`,
        model: 'error'
      };
    }
  }
}
