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
import { Events } from '../../../core/shared/Events';
import { COLLECTIONS } from '../../../shared/Constants';
import type { ChatMessageEntity } from '../../../data/entities/ChatMessageEntity';
import type { UserEntity } from '../../../data/entities/UserEntity';
import type { RoomEntity } from '../../../data/entities/RoomEntity';
import { CognitionLogger } from './cognition/CognitionLogger';
import type { Task } from './cognition/reasoning/types';
import { ChatRAGBuilder } from '../../../rag/builders/ChatRAGBuilder';
import { CoordinationDecisionLogger, type LogDecisionParams } from '../../../coordination/server/CoordinationDecisionLogger';
import type { RAGContext } from '../../../data/entities/CoordinationDecisionEntity';
import type { AIDecisionContext } from '../../../ai/server/AIDecisionService';
import { AIDecisionService } from '../../../ai/server/AIDecisionService';
import type { DecisionContext } from './cognition/adapters/IDecisionAdapter';
import { getChatCoordinator } from '../../../coordination/server/ChatCoordinationStream';
import { calculateMessagePriority } from './PersonaInbox';
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
 * PersonaMessageEvaluator - Message evaluation and response decision engine
 *
 * Handles:
 * - Cognition-based response planning (with SelfState, WorkingMemory)
 * - Message gating (should respond?)
 * - Response coordination (with other AIs)
 * - Heuristic scoring and fallbacks
 */
export class PersonaMessageEvaluator {
  constructor(private readonly personaUser: PersonaUser) {}

  /**
   * Log to persona's cognition.log file
   */
  private log(message: string, ...args: any[]): void {
    const timestamp = new Date().toISOString();
    const formattedArgs = args.length > 0 ? ' ' + args.map(a => String(a)).join(' ') : '';
    this.personaUser.logger.enqueueLog('cognition.log', `[${timestamp}] ${message}${formattedArgs}\n`);
  }

  /**
   * Evaluate message with full cognition system (planning, focus, working memory)
   *
   * PHASE 4: Cognition wrapper around evaluateAndPossiblyRespond
   * Creates Task → Plan → Updates SelfState → Executes → Logs to CognitionLogger
   */
  async evaluateAndPossiblyRespondWithCognition(
    messageEntity: ChatMessageEntity,
    senderIsHuman: boolean,
    messageText: string
  ): Promise<void> {
    const taskStartTime = Date.now();

    // STEP 1: Create Task from message
    const task: Task = {
      id: `task-${messageEntity.id}` as UUID,
      domain: 'chat',
      contextId: messageEntity.roomId,
      description: `Respond to: "${messageText.slice(0, 100)}"`,
      priority: calculateMessagePriority(
        {
          content: messageText,
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
      thoughtContent: `Received message from ${messageEntity.senderName}: "${messageText.slice(0, 200)}"`,
      importance: task.priority,
      shareable: false
    });

    // STEP 5: Execute plan steps (existing chat logic inside)
    try {
      // Mark step 1 complete: "Recall relevant context"
      plan.steps[0].completed = true;
      plan.steps[0].completedAt = Date.now();

      // Execute step 2: "Generate thoughtful response" (existing logic)
      await this.evaluateAndPossiblyRespond(messageEntity, senderIsHuman, messageText);

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
    messageEntity: ChatMessageEntity,
    senderIsHuman: boolean,
    messageText: string
  ): Promise<void> {
    // STEP 2: Check response cap (prevent infinite loops)
    if (this.personaUser.rateLimiter.hasReachedResponseCap(messageEntity.roomId)) {
      const currentCount = this.personaUser.rateLimiter.getResponseCount(messageEntity.roomId);
      const config = this.personaUser.rateLimiter.getConfig();
      this.personaUser.logAIDecision('SILENT', `Response cap reached (${currentCount}/${config.maxResponsesPerSession})`, {
        message: messageText,
        sender: messageEntity.senderName,
        roomId: messageEntity.roomId
      });
      return;
    }

    // STEP 3: Check if mentioned
    const isMentioned = this.isPersonaMentioned(messageText);

    // STEP 4: Check rate limiting (before expensive LLM call)
    if (this.personaUser.rateLimiter.isRateLimited(messageEntity.roomId)) {
      const info = this.personaUser.rateLimiter.getRateLimitInfo(messageEntity.roomId);
      this.personaUser.logAIDecision('SILENT', `Rate limited, wait ${info.waitTimeSeconds?.toFixed(1)}s more`, {
        message: messageText,
        sender: messageEntity.senderName,
        roomId: messageEntity.roomId
      });
      return;
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
          messagePreview: messageText.slice(0, 100),
          senderName: messageEntity.senderName
        },
        {
          scope: EVENT_SCOPES.ROOM,
          scopeId: messageEntity.roomId,
        }
      );
    }

    const gatingResult = await this.evaluateShouldRespond(messageEntity, senderIsHuman, isMentioned);

    // FULL TRANSPARENCY LOGGING
    this.log(`\n${'='.repeat(80)}`);
    this.log(`🧠 ${this.personaUser.displayName}: GATING DECISION for message "${messageText.slice(0, 60)}..."`);
    this.log(`${'='.repeat(80)}`);
    this.log(`📊 Context: ${gatingResult.ragContextSummary?.filteredMessages ?? 0} messages in ${gatingResult.ragContextSummary?.timeWindowMinutes ?? 0}min window`);
    this.log(`💬 Conversation history seen by AI:`);
    gatingResult.conversationHistory?.slice(-5).forEach((msg, i) => {
      this.log(`   ${i + 1}. [${msg.name}] ${msg.content.slice(0, 80)}...`);
    });
    this.log(`\n🎯 Decision: ${gatingResult.shouldRespond ? 'RESPOND' : 'SILENT'}`);
    this.log(`   Confidence: ${(gatingResult.confidence * 100).toFixed(0)}%`);
    this.log(`   Reason: ${gatingResult.reason}`);
    this.log(`   Model: ${gatingResult.model}`);
    this.log(`${'='.repeat(80)}\n`);

    if (!gatingResult.shouldRespond) {
      // PHASE 5C: Log coordination decision to database (fire-and-forget)
      if (gatingResult.filteredRagContext) {
        const decisionStartTime = Date.now();
        const ragContext = this.buildCoordinationRAGContext(gatingResult.filteredRagContext);

        // Fire-and-forget: Don't await, don't slow down critical path
        CoordinationDecisionLogger.logDecision({
          actorId: this.personaUser.id,
          actorName: this.personaUser.displayName,
          actorType: 'ai-persona',
          triggerEventId: messageEntity.id,
          ragContext,
          visualContext: undefined,
          action: 'SILENT',
          confidence: gatingResult.confidence,
          reasoning: gatingResult.reason,
          responseContent: undefined,
          modelUsed: gatingResult.model,
          modelProvider: this.personaUser.modelConfig.provider ?? 'ollama',
          tokensUsed: undefined,
          responseTime: Date.now() - decisionStartTime,
          sessionId: DataDaemon.jtagContext!.uuid,
          contextId: messageEntity.roomId,
          tags: [senderIsHuman ? 'human-sender' : 'ai-sender', 'gating-silent']
        }).catch(error => {
          this.log(`❌ ${this.personaUser.displayName}: Failed to log SILENT decision:`, error);
        });
      }

      this.personaUser.logAIDecision('SILENT', gatingResult.reason, {
        message: messageText,
        sender: messageEntity.senderName,
        roomId: messageEntity.roomId,
        confidence: gatingResult.confidence,
        model: gatingResult.model,
        ragContextSummary: gatingResult.ragContextSummary,
        conversationHistory: gatingResult.conversationHistory
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
            confidence: gatingResult.confidence ?? 0.5,
            reason: gatingResult.reason,
            gatingModel: gatingResult.model ?? 'unknown'
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
    const decisionContext = gatingResult.filteredRagContext ? {
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
      modelProvider: this.personaUser.modelConfig.provider ?? 'ollama',
      sessionId: DataDaemon.jtagContext!.uuid,
      contextId: messageEntity.roomId,
      tags: [
        senderIsHuman ? 'human-sender' : 'ai-sender',
        isMentioned ? 'mentioned' : 'not-mentioned',
        'gating-respond'
      ]
    } : undefined;

    this.personaUser.logAIDecision('RESPOND', gatingResult.reason, {
      message: messageText,
      sender: messageEntity.senderName,
      roomId: messageEntity.roomId,
      mentioned: isMentioned,
      humanSender: senderIsHuman,
      confidence: gatingResult.confidence,
      model: gatingResult.model,
      ragContextSummary: gatingResult.ragContextSummary,
      conversationHistory: gatingResult.conversationHistory
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
          confidence: gatingResult.confidence ?? 0.5,
          reason: gatingResult.reason,
          gatingModel: gatingResult.model ?? 'unknown'
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

      // TODO: Ask AI if they still want to respond given the new messages
      // For now, just log and proceed (future: implement smart rejection)
      this.log(`   New messages: ${newMessages.map(m => `[${m.data.senderName}] ${m.data.content.text.slice(0, 50)}`).join(', ')}`);
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
    await this.personaUser.respondToMessage(messageEntity, decisionContext);
    this.log(`🔧 TRACE-POINT-C: After respondToMessage returned (timestamp=${Date.now()})`);
    this.log(`✅ ${this.personaUser.displayName}: [PHASE 3/3] Response posted successfully`);

    // PHASE 3BIS: Notify coordinator that message was serviced (lowers temperature)
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
  private buildCoordinationRAGContext(filteredRagContext: any): RAGContext {
    const systemPrompt = filteredRagContext.identity?.systemPrompt ??
                         `You are ${this.personaUser.displayName}. ${this.personaUser.entity?.bio ?? ''}`;

    return {
      identity: {
        systemPrompt,
        bio: this.personaUser.entity?.bio ?? '',
        role: this.personaUser.displayName
      },
      conversationHistory: (filteredRagContext.conversationHistory ?? []).map((msg: any) => ({
        role: msg.role,
        content: msg.content,
        timestamp: msg.timestamp ?? Date.now()
      })),
      artifacts: filteredRagContext.artifacts ?? [],
      privateMemories: filteredRagContext.privateMemories ?? [],
      metadata: {
        timestamp: Date.now(),
        tokenCount: filteredRagContext.metadata?.messageCount ??
                    filteredRagContext.conversationHistory?.length ?? 0,
        contextWindow: 4096
      }
    };
  }

  /**
   * Check if this persona is mentioned in a message
   * Supports @username mentions and channel directives
   *
   * TODO Phase 2: Use dedicated mention/directive events instead of text parsing
   */
  private isPersonaMentioned(messageText: string): boolean {
    const messageTextLower = messageText.toLowerCase();
    const displayNameLower = this.personaUser.displayName.toLowerCase();
    const uniqueIdLower = this.personaUser.entity.uniqueId?.toLowerCase() || '';

    // Check for @mentions ANYWHERE in message: "@PersonaName" or "@uniqueid"
    // Works like Discord/Slack - @ can be at start, middle, or end
    if (messageTextLower.includes(`@${displayNameLower}`) ||
        messageTextLower.includes(`@${uniqueIdLower}`)) {
      return true;
    }

    // Check for direct address at START: "PersonaName," or "PersonaName:"
    // e.g. "Teacher AI, explain closures" or "teacher-ai: what's up"
    if (messageTextLower.startsWith(displayNameLower + ',') ||
        messageTextLower.startsWith(displayNameLower + ':') ||
        messageTextLower.startsWith(uniqueIdLower + ',') ||
        messageTextLower.startsWith(uniqueIdLower + ':')) {
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
  private async calculateResponseHeuristics(messageEntity: ChatMessageEntity): Promise<{
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
      const result = await DataDaemon.read<UserEntity>(COLLECTIONS.USERS, senderId);

      if (!result.success || !result.data) {
        this.log(`⚠️  PersonaUser ${this.personaUser.displayName}: Could not read sender ${senderId}, BLOCKING response`);
        return false; // Fail CLOSED - don't respond if database fails (prevents loops)
      }

      const senderType = result.data.data.type;
      return senderType === 'human';

    } catch (error: any) {
      this.log(`❌ PersonaUser ${this.personaUser.displayName}: Error checking sender type, BLOCKING response:`, error);
      return false; // Fail CLOSED on error (prevents loops)
    }
  }

  /**
   * Evaluate whether to respond using Decision Adapter Chain
   *
   * PHASE 6: Refactored to use adapter pattern (fast-path, thermal, LLM)
   * Instead of hardcoded logic, delegates to chain of decision adapters.
   */
  async evaluateShouldRespond(
    message: ChatMessageEntity,
    senderIsHuman: boolean,
    isMentioned: boolean
  ): Promise<{
    shouldRespond: boolean;
    confidence: number;
    reason: string;
    model?: string;
    ragContextSummary?: {
      totalMessages: number;
      filteredMessages: number;
      timeWindowMinutes?: number;
    };
    conversationHistory?: Array<{
      name: string;
      content: string;
      timestamp?: number;
    }>;
    filteredRagContext?: any;
  }> {
    const startTime = Date.now();

    try {
      // PHASE 6: Use Decision Adapter Chain for all decisions
      const context: DecisionContext<ChatMessageEntity> = {
        triggerEvent: message,
        eventContent: message.content.text,
        personaId: this.personaUser.id,
        personaDisplayName: this.personaUser.displayName,
        senderIsHuman,
        isMentioned,
        gatingModel: (this.personaUser.entity?.personaConfig as any)?.gatingModel,
        contextWindowMinutes: (this.personaUser.entity?.personaConfig as any)?.contextWindowMinutes ?? 30,
        minContextMessages: (this.personaUser.entity?.personaConfig as any)?.minContextMessages ?? 15
      };

      const decision = await this.personaUser.decisionChain.processDecision(context);

      this.log(`🔗 ${this.personaUser.displayName}: Adapter decision: ${decision.shouldRespond ? 'RESPOND' : 'SILENT'} via ${decision.model}`);

      // Build RAG context for decision logging (all adapters need this)
      const ragBuilder = new ChatRAGBuilder();
      const ragContext = await ragBuilder.buildContext(
        message.roomId,
        this.personaUser.id,
        {
          modelId: decision.model,  // Bug #5 fix: Pass model ID for dynamic message count calculation
          maxMemories: 0,
          includeArtifacts: false,
          includeMemories: false,
          currentMessage: {
            role: 'user',
            content: message.content.text,
            name: message.senderName,
            timestamp: this.personaUser.timestampToNumber(message.timestamp)
          }
        }
      );

      return {
        shouldRespond: decision.shouldRespond,
        confidence: decision.confidence,
        reason: decision.reason,
        model: decision.model,
        filteredRagContext: ragContext,
        ragContextSummary: {
          totalMessages: ragContext.conversationHistory.length,
          filteredMessages: ragContext.conversationHistory.length,
          timeWindowMinutes: context.contextWindowMinutes
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

      return {
        shouldRespond: isMentioned,
        confidence: isMentioned ? (0.92 + Math.random() * 0.06) : 0.5, // 0.92-0.98 realistic range
        reason: 'Error in evaluation',
        model: 'error'
      };
    }
  }
}
