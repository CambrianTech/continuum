/**
 * PersonaUser - Internal AI citizen with RAG + optional LoRA genome
 *
 * THE ACTUAL AI THAT RESPONDS TO CHAT MESSAGES.
 * This is what makes the system useful - without this, we're chatting with the void.
 *
 * Architecture:
 * - Subscribes to data:ChatMessage:created events
 * - Loads chat history as RAG context
 * - Calls AI API (Claude/GPT) to generate responses
 * - Posts responses back to chat as this persona
 * - Dedicated SQLite storage per persona
 */

import { AIUser } from '../shared/AIUser';
import { UserEntity } from '../../data/entities/UserEntity';
import { UserStateEntity } from '../../data/entities/UserStateEntity';
import type { IUserStateStorage } from '../storage/IUserStateStorage';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { JTAGContext } from '../../core/types/JTAGTypes';
import type { JTAGRouter } from '../../core/router/shared/JTAGRouter';
import { Commands } from '../../core/shared/Commands';
import { DATA_COMMANDS } from '../../../commands/data/shared/DataCommandConstants';
import type { JTAGClient } from '../../core/client/shared/JTAGClient';
import { ChatMessageEntity } from '../../data/entities/ChatMessageEntity';
import type { RoomEntity } from '../../data/entities/RoomEntity';
import type { UserCreateParams } from '../../../commands/user/create/shared/UserCreateTypes';
import type { DataCreateParams, DataCreateResult } from '../../../commands/data/create/shared/DataCreateTypes';
import type { DataReadParams, DataReadResult } from '../../../commands/data/read/shared/DataReadTypes';
import type { DataUpdateParams, DataUpdateResult } from '../../../commands/data/update/shared/DataUpdateTypes';
import type { Thought, ThoughtType } from '../../conversation/shared/ConversationCoordinationTypes';
import { getThoughtStreamCoordinator } from '../../conversation/server/ThoughtStreamCoordinator';
import { MemoryStateBackend } from '../storage/MemoryStateBackend';
import { getDefaultCapabilitiesForType, getDefaultPreferencesForType } from '../config/UserCapabilitiesDefaults';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../data/config/DatabaseConfig';
import { AIProviderDaemon } from '../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import type { TextGenerationRequest, TextGenerationResponse } from '../../../daemons/ai-provider-daemon/shared/AIProviderTypes';
import { ChatRAGBuilder } from '../../rag/builders/ChatRAGBuilder';
import type { ShouldRespondFastParams, ShouldRespondFastResult } from '../../../commands/ai/should-respond-fast/shared/ShouldRespondFastTypes';
import type { AIShouldRespondParams, AIShouldRespondResult } from '../../../commands/ai/should-respond/shared/AIShouldRespondTypes';
import type { GenomeEntity } from '../../genome/entities/GenomeEntity';
import { AIDecisionLogger } from '../../ai/server/AIDecisionLogger';
import { AIDecisionService, type AIDecisionContext } from '../../ai/server/AIDecisionService';
import {
  AI_DECISION_EVENTS,
  type AIEvaluatingEventData,
  type AIDecidedRespondEventData,
  type AIDecidedSilentEventData,
  type AIGeneratingEventData,
  type AICheckingRedundancyEventData,
  type AIPostedEventData,
  type AIErrorEventData
} from '../../events/shared/AIDecisionEvents';
import type { ScopedEventsInterface } from '../../events/shared/ScopedEventSystem';

/**
 * RAG Context Types - Storage structure for persona conversation context
 */
interface PersonaRAGMessage {
  senderId: UUID;
  senderName: string;
  text: string;
  timestamp: string;
}

interface PersonaRAGContext {
  roomId: UUID;
  personaId: UUID;
  messages: PersonaRAGMessage[];
  lastUpdated: string;
  tokenCount: number;
}

/**
 * PersonaUser - Our internal AI citizens
 *
 * First-class citizens with their own JTAGClient for universal Commands/Events API
 */
export class PersonaUser extends AIUser {
  private isInitialized: boolean = false;
  private eventsSubscribed: boolean = false;
  // Note: client is now in BaseUser as protected property, accessible via this.client
  // ArtifactsAPI access is through this.client.daemons.artifacts

  // Rate limiting state (in-memory for now, will move to SQLite later)
  private lastResponseTime: Map<UUID, Date> = new Map();
  private readonly minSecondsBetweenResponses = 10; // 10 seconds between responses per room

  // Response cap to prevent infinite loops
  private responseCount: Map<UUID, number> = new Map(); // room -> count
  private readonly maxResponsesPerSession = 50; // Max 50 responses per room per session

  constructor(
    entity: UserEntity,
    state: UserStateEntity,
    storage: IUserStateStorage,
    client?: JTAGClient
  ) {
    super(entity, state, storage, client); // ✅ Pass client to BaseUser for event subscriptions
  }

  /**
   * Log AI decision to dedicated AI log (separate from general system logs)
   * Uses AIDecisionLogger to write to .continuum/jtag/sessions/system/{sessionId}/logs/ai-decisions.log
   */
  private logAIDecision(
    decision: 'RESPOND' | 'SILENT',
    reason: string,
    context: {
      message: string;
      sender: string;
      roomId: string;
      mentioned?: boolean;
      humanSender?: boolean;
      confidence?: number;
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
    }
  ): void {
    AIDecisionLogger.logDecision(this.displayName, decision, reason, context);
  }

  /**
   * Initialize persona - use BaseUser helpers for room management and event subscriptions
   * PersonaUser acts as an autonomous agent, listening for messages in rooms they're a member of
   */
  async initialize(): Promise<void> {
    if (this.isInitialized) {
      // CRITICAL: Reload rooms even when already initialized
      // PersonaUsers might be created before rooms exist, so we need to refresh membership
      await this.loadMyRooms();

      // ✅ FIX: Do NOT re-subscribe - event handlers persist across reinit
      // Re-subscribing creates duplicate handlers (memory leak)
      return;
    }

    // STEP 1: Base initialization (loads state + rooms)
    await super.initialize();

    // STEP 2: Subscribe to room-specific chat events (only if client available)
    if (this.client && !this.eventsSubscribed) {
      // Subscribe to ALL chat events once (not per-room)
      // subscribeToChatEvents() filters by this.myRoomIds internally
      this.subscribeToChatEvents(this.handleChatMessage.bind(this));
      this.subscribeToRoomUpdates(this.handleRoomUpdate.bind(this));

      // Subscribe to truncate events to cancel in-flight processing
      this.client.daemons.events.on('data:chat_messages:truncated', () => {
        this.responseCount.clear();
        this.lastResponseTime.clear();
      });

      this.eventsSubscribed = true;
    }

    this.isInitialized = true;
  }

  /**
   * Handle incoming chat message - THOUGHT STREAM COORDINATION
   * RTOS-inspired: Broadcast thoughts, observe others, coordinate naturally
   */
  private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {
    // STEP 1: Ignore our own messages
    if (messageEntity.senderId === this.id) {
      return;
    }

    const senderIsHuman = messageEntity.senderType === 'human';
    const messageText = messageEntity.content?.text || '';

    // === SEQUENTIAL EVALUATION: Request turn (brain-like, one at a time) ===
    const coordinator = getThoughtStreamCoordinator();
    const releaseTurn = await coordinator.requestEvaluationTurn(messageEntity.id, this.id);

    try {
      await this.evaluateAndPossiblyRespond(messageEntity, senderIsHuman, messageText);
    } catch (error) {
      // 🚨 CRITICAL: Log errors instead of silent failure
      const errorMessage = error instanceof Error ? error.message : String(error);
      const errorStack = error instanceof Error ? error.stack : undefined;

      console.error(`❌ ${this.displayName}: ERROR during evaluation/response:`, {
        error: errorMessage,
        stack: errorStack,
        messageId: messageEntity.id,
        roomId: messageEntity.roomId,
        sender: messageEntity.senderName
      });

      // Emit ERROR event for diagnostics
      if (this.client) {
        (this.client.events as unknown as ScopedEventsInterface).room(messageEntity.roomId).emit(AI_DECISION_EVENTS.ERROR, {
          personaId: this.id,
          personaName: this.displayName,
          roomId: messageEntity.roomId,
          messageId: messageEntity.id,
          isHumanMessage: senderIsHuman,
          timestamp: Date.now(),
          error: errorMessage,
          phase: 'evaluating'  // Error happened during evaluation/response phase
        } as AIErrorEventData);
      }

      // Log to AI decisions log
      const operation = `Evaluation/Response for message from ${messageEntity.senderName} in room ${messageEntity.roomId}`;
      const errorDetails = `${errorMessage}${errorStack ? '\n' + errorStack.split('\n').slice(0, 5).join('\n') : ''}`;
      AIDecisionLogger.logError(this.displayName, operation, errorDetails);
    } finally {
      releaseTurn(); // Always release turn, even if evaluation fails
    }
  }

  /**
   * Evaluate message and possibly respond (called with exclusive evaluation lock)
   */
  private async evaluateAndPossiblyRespond(
    messageEntity: ChatMessageEntity,
    senderIsHuman: boolean,
    messageText: string
  ): Promise<void> {
    // STEP 2: Check response cap (prevent infinite loops)
    const currentCount = this.responseCount.get(messageEntity.roomId) || 0;
    if (currentCount >= this.maxResponsesPerSession) {
      this.logAIDecision('SILENT', `Response cap reached (${currentCount}/${this.maxResponsesPerSession})`, {
        message: messageText,
        sender: messageEntity.senderName,
        roomId: messageEntity.roomId
      });
      return;
    }

    // STEP 3: Check if mentioned
    const isMentioned = this.isPersonaMentioned(messageText);

    // STEP 4: Check rate limiting (before expensive LLM call)
    if (this.isRateLimited(messageEntity.roomId)) {
      const lastTime = this.lastResponseTime.get(messageEntity.roomId)!;
      const secondsSince = (Date.now() - lastTime.getTime()) / 1000;
      const waitTime = this.minSecondsBetweenResponses - secondsSince;
      this.logAIDecision('SILENT', `Rate limited, wait ${waitTime.toFixed(1)}s more`, {
        message: messageText,
        sender: messageEntity.senderName,
        roomId: messageEntity.roomId
      });
      return;
    }

    // === EVALUATE: Use LLM-based intelligent gating to decide if should respond ===
    // Emit EVALUATING event for real-time feedback
    if (this.client) {
      (this.client.events as unknown as ScopedEventsInterface).room(messageEntity.roomId).emit(AI_DECISION_EVENTS.EVALUATING, {
        personaId: this.id,
        personaName: this.displayName,
        roomId: messageEntity.roomId,
        messageId: messageEntity.id,
        isHumanMessage: senderIsHuman,
        timestamp: Date.now(),
        messagePreview: messageText.slice(0, 100),
        senderName: messageEntity.senderName
      } as AIEvaluatingEventData);
    }

    const gatingResult = await this.evaluateShouldRespond(messageEntity, senderIsHuman, isMentioned);

    if (!gatingResult.shouldRespond) {
      this.logAIDecision('SILENT', gatingResult.reason, {
        message: messageText,
        sender: messageEntity.senderName,
        roomId: messageEntity.roomId,
        confidence: gatingResult.confidence,
        model: gatingResult.model,
        ragContextSummary: gatingResult.ragContextSummary,
        conversationHistory: gatingResult.conversationHistory
      });

      // Emit DECIDED_SILENT event
      if (this.client) {
        (this.client.events as unknown as ScopedEventsInterface).room(messageEntity.roomId).emit(AI_DECISION_EVENTS.DECIDED_SILENT, {
          personaId: this.id,
          personaName: this.displayName,
          roomId: messageEntity.roomId,
          messageId: messageEntity.id,
          isHumanMessage: senderIsHuman,
          timestamp: Date.now(),
          confidence: gatingResult.confidence ?? 0.5,
          reason: gatingResult.reason,
          gatingModel: gatingResult.model ?? 'unknown'
        } as AIDecidedSilentEventData);
      }

      return;
    }

    // === RESPOND: LLM gating decided to respond, coordinate with other AIs ===
    this.logAIDecision('RESPOND', gatingResult.reason, {
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
    if (this.client) {
      (this.client.events as unknown as ScopedEventsInterface).room(messageEntity.roomId).emit(AI_DECISION_EVENTS.DECIDED_RESPOND, {
        personaId: this.id,
        personaName: this.displayName,
        roomId: messageEntity.roomId,
        messageId: messageEntity.id,
        isHumanMessage: senderIsHuman,
        timestamp: Date.now(),
        confidence: gatingResult.confidence ?? 0.5,
        reason: gatingResult.reason,
        gatingModel: gatingResult.model ?? 'unknown'
      } as AIDecidedRespondEventData);
    }

    // === COORDINATION: Broadcast "claiming" thought and wait for permission ===
    const coordinator = getThoughtStreamCoordinator();
    const thought: Thought = {
      personaId: this.id,
      type: 'claiming',
      confidence: gatingResult.confidence ?? 0.5,
      reasoning: gatingResult.reason,
      timestamp: new Date()
    };

    await this.broadcastThought(messageEntity.id, thought);

    // Wait for coordinator decision (fast: typically <100ms with early exit rules)
    const decision = await coordinator.waitForDecision(messageEntity.id, 3000);

    // Check if we were granted permission to respond
    if (!decision || !decision.granted.includes(this.id)) {
      this.logAIDecision('SILENT', 'ThoughtStreamCoordinator denied (higher confidence AI responding)', {
        message: messageText,
        sender: messageEntity.senderName,
        roomId: messageEntity.roomId,
        confidence: gatingResult.confidence
      });
      return; // Don't generate - let higher confidence AI respond
    }

    console.log(`✅ ${this.displayName}: Granted permission by coordinator (conf=${gatingResult.confidence})`);

    // 🔧 PHASE: Update RAG context
    console.log(`🔧 ${this.displayName}: [PHASE 1/3] Updating RAG context...`);
    await this.updateRAGContext(messageEntity.roomId, messageEntity);
    console.log(`✅ ${this.displayName}: [PHASE 1/3] RAG context updated`);

    // 🔧 PHASE: Emit GENERATING event
    console.log(`🔧 ${this.displayName}: [PHASE 2/3] Emitting GENERATING event...`);
    if (this.client) {
      (this.client.events as unknown as ScopedEventsInterface).room(messageEntity.roomId).emit(AI_DECISION_EVENTS.GENERATING, {
        personaId: this.id,
        personaName: this.displayName,
        roomId: messageEntity.roomId,
        messageId: messageEntity.id,
        isHumanMessage: senderIsHuman,
        timestamp: Date.now(),
        responseModel: this.entity?.personaConfig?.responseModel ?? 'default'
      } as AIGeneratingEventData);
    }
    console.log(`✅ ${this.displayName}: [PHASE 2/3] GENERATING event emitted`);

    // 🔧 PHASE: Generate and post response
    console.log(`🔧 ${this.displayName}: [PHASE 3/3] Calling respondToMessage...`);
    await this.respondToMessage(messageEntity);
    console.log(`✅ ${this.displayName}: [PHASE 3/3] Response posted successfully`);


    // Increment response count
    const newCount = (this.responseCount.get(messageEntity.roomId) || 0) + 1;
    this.responseCount.set(messageEntity.roomId, newCount);

    // Track response time for rate limiting
    this.lastResponseTime.set(messageEntity.roomId, new Date());
  }

  /**
   * Check if this persona is rate limited for a room
   */
  private isRateLimited(roomId: UUID): boolean {
    const lastTime = this.lastResponseTime.get(roomId);
    if (!lastTime) {
      return false; // Never responded in this room
    }

    const secondsSince = (Date.now() - lastTime.getTime()) / 1000;
    return secondsSince < this.minSecondsBetweenResponses;
  }

  /**
   * Convert timestamp to number (handles Date, number, or undefined from JSON serialization)
   */
  private timestampToNumber(timestamp: Date | number | undefined): number {
    if (timestamp === undefined) {
      return Date.now(); // Use current time if timestamp missing
    }
    return timestamp instanceof Date ? timestamp.getTime() : timestamp;
  }

  /**
   * Self-review: Check if generated response is redundant compared to conversation history
   * Like a human who drafts a response, re-reads the chat, and thinks "oh someone already said that"
   */
  private async isResponseRedundant(
    myResponse: string,
    roomId: UUID,
    conversationHistory: Array<{ role: string; content: string; name?: string; timestamp?: number }>
  ): Promise<boolean> {
    try {
      // Use AIDecisionService for centralized redundancy checking
      // Create minimal context without needing full trigger message
      const decisionContext: AIDecisionContext = {
        personaId: this.id,
        personaName: this.displayName,
        roomId,
        triggerMessage: {
          id: '',
          roomId,
          senderId: '',
          senderName: 'System',
          senderType: 'system',
          content: { text: 'redundancy check', attachments: [] },
          timestamp: new Date(),
          collection: COLLECTIONS.CHAT_MESSAGES,
          version: 1,
          createdAt: new Date(),
          updatedAt: new Date(),
          status: 'sent',
          priority: 0,
          reactions: []
        } as unknown as ChatMessageEntity,
        ragContext: {
          domain: 'chat',
          contextId: roomId,
          personaId: this.id,
          identity: {
            name: this.displayName,
            systemPrompt: ''
          },
          conversationHistory: conversationHistory.map(msg => ({
            role: msg.role as 'user' | 'assistant',
            content: msg.content,
            name: msg.name,
            timestamp: msg.timestamp
          })),
          artifacts: [],
          privateMemories: [],
          metadata: {
            messageCount: conversationHistory.length,
            artifactCount: 0,
            memoryCount: 0,
            builtAt: new Date()
          }
        }
      };

      const result = await AIDecisionService.checkRedundancy(
        myResponse,
        decisionContext,
        { model: 'llama3.2:3b' }
      );

      return result.isRedundant;
    } catch (error) {
      AIDecisionLogger.logError(this.displayName, 'Redundancy check', error instanceof Error ? error.message : String(error));
      return false; // On error, allow the response (fail open)
    }
  }

  /**
   * Generate and post a response to a chat message
   * Phase 2: AI-powered responses with RAG context via AIProviderDaemon
   */
  private async respondToMessage(originalMessage: ChatMessageEntity): Promise<void> {
    try {
      // 🔧 SUB-PHASE 3.1: Build RAG context
      console.log(`🔧 ${this.displayName}: [PHASE 3.1] Building RAG context...`);
      const ragBuilder = new ChatRAGBuilder();
      const fullRAGContext = await ragBuilder.buildContext(
        originalMessage.roomId,
        this.id,
        {
          maxMessages: 20,
          maxMemories: 10,
          includeArtifacts: false, // Skip artifacts for now (image attachments)
          includeMemories: false,   // Skip private memories for now
          // ✅ FIX: Include current message even if not yet persisted to database
          currentMessage: {
            role: 'user',
            content: originalMessage.content.text,
            name: originalMessage.senderName,
            timestamp: this.timestampToNumber(originalMessage.timestamp)
          }
        }
      );
      console.log(`✅ ${this.displayName}: [PHASE 3.1] RAG context built (${fullRAGContext.conversationHistory.length} messages)`);

      // 🔧 SUB-PHASE 3.2: Build message history for LLM
      console.log(`🔧 ${this.displayName}: [PHASE 3.2] Building LLM message array...`);
      const messages: Array<{ role: 'system' | 'user' | 'assistant'; content: string }> = [];

      // System prompt from RAG builder (includes room membership!)
      messages.push({
        role: 'system',
        content: fullRAGContext.identity.systemPrompt
      });

      // Add conversation history from RAG context with human-readable timestamps
      // NOTE: Llama 3.2 doesn't support multi-party chats natively, so we embed speaker names in content
      // Format: "[HH:MM] SpeakerName: message" - timestamps help LLM understand time gaps
      if (fullRAGContext.conversationHistory.length > 0) {
        let lastTimestamp: number | undefined;

        for (let i = 0; i < fullRAGContext.conversationHistory.length; i++) {
          const msg = fullRAGContext.conversationHistory[i];

          // Format timestamp as human-readable time
          let timePrefix = '';
          if (msg.timestamp) {
            const date = new Date(msg.timestamp);
            const hours = date.getHours().toString().padStart(2, '0');
            const minutes = date.getMinutes().toString().padStart(2, '0');
            timePrefix = `[${hours}:${minutes}] `;

            // Detect significant time gaps (> 1 hour)
            if (lastTimestamp && (msg.timestamp - lastTimestamp > 3600000)) {
              const gapHours = Math.floor((msg.timestamp - lastTimestamp) / 3600000);
              messages.push({
                role: 'system',
                content: `⏱️ ${gapHours} hour${gapHours > 1 ? 's' : ''} passed - conversation resumed`
              });
            }

            lastTimestamp = msg.timestamp;
          }

          // For Llama models, embed speaker identity + timestamp in the content
          const formattedContent = msg.name
            ? `${timePrefix}${msg.name}: ${msg.content}`
            : `${timePrefix}${msg.content}`;

          messages.push({
            role: msg.role,
            content: formattedContent
          });
        }
      }

      // CRITICAL: Identity reminder at END of context (research shows this prevents "prompt drift")
      // LLMs have recency bias - instructions at the end have MORE influence than at beginning
      // This prevents the persona from copying the "Name: message" format or inventing fake participants
      const now = new Date();
      const currentTime = `${now.toLocaleDateString('en-US', { month: '2-digit', day: '2-digit', year: 'numeric' })} ${now.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', hour12: false })}`;

      messages.push({
        role: 'system',
        content: `IDENTITY REMINDER: You are ${this.displayName}. Respond naturally with JUST your message - NO name prefix, NO "A:" or "H:" labels, NO fake conversations. The room has ONLY these people: ${fullRAGContext.identity.systemPrompt.match(/Current room members: ([^\n]+)/)?.[1] || 'unknown members'}.

CURRENT TIME: ${currentTime}

IMPORTANT: Pay attention to the timestamps in brackets [HH:MM]. If messages are from hours ago but the current question is recent, the conversation topic likely changed. Focus your response on the MOST RECENT message, not old topics.`
      });
      console.log(`✅ ${this.displayName}: [PHASE 3.2] LLM message array built (${messages.length} messages)`);

      // 🔧 SUB-PHASE 3.3: Generate AI response with timeout
      console.log(`🔧 ${this.displayName}: [PHASE 3.3] Calling AIProviderDaemon.generateText (model: llama3.2:3b)...`);
      const request: TextGenerationRequest = {
        messages,
        model: 'llama3.2:3b', // Larger model for better instruction following (was 1b)
        temperature: 0.7,
        maxTokens: 150, // Keep responses concise
        preferredProvider: 'ollama'
      };

      // Wrap generation call with timeout (45s - reasonable limit for local Ollama generation)
      // Queue can handle 4 concurrent requests, so 45s is sufficient
      const GENERATION_TIMEOUT_MS = 45000;
      const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => reject(new Error('AI generation timeout after 45 seconds')), GENERATION_TIMEOUT_MS);
      });

      let aiResponse: TextGenerationResponse;
      try {
        aiResponse = await Promise.race([
          AIProviderDaemon.generateText(request),
          timeoutPromise
        ]);
        console.log(`✅ ${this.displayName}: [PHASE 3.3] AI response generated (${aiResponse.text.trim().length} chars)`);
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        console.error(`❌ ${this.displayName}: [PHASE 3.3] AI generation failed:`, errorMessage);

        // Emit ERROR event for UI display
        if (this.client) {
          (this.client.events as unknown as ScopedEventsInterface).room(originalMessage.roomId).emit(AI_DECISION_EVENTS.ERROR, {
            personaId: this.id,
            personaName: this.displayName,
            roomId: originalMessage.roomId,
            messageId: originalMessage.id,
            isHumanMessage: originalMessage.senderType === 'human',
            timestamp: Date.now(),
            error: errorMessage,
            phase: 'generating'
          } as AIErrorEventData);
        }

        // Log error to AI decisions log
        AIDecisionLogger.logError(this.displayName, 'AI generation (PHASE 3.3)', errorMessage);

        // Re-throw to be caught by outer try-catch
        throw error;
      }

      // === SUB-PHASE 3.4: SELF-REVIEW: Check if response is redundant before posting ===
      // DISABLED: Redundancy checking via LLM is too flaky (false positives like C++ vs JavaScript questions)
      // It adds AI unreliability on top of AI unreliability, leading to valid responses being discarded
      // TODO: Replace with simple heuristics (exact text match, time-based deduplication)
      console.log(`⏭️  ${this.displayName}: [PHASE 3.4] Redundancy check DISABLED (too flaky), proceeding to post`);
      const isRedundant = false; // Disabled

      // Old flaky code (commented out):
      /*
      console.log(`🔧 ${this.displayName}: [PHASE 3.4] Checking redundancy...`);
      // Emit CHECKING_REDUNDANCY event
      if (this.client) {
        (this.client.events as unknown as ScopedEventsInterface).room(originalMessage.roomId).emit(AI_DECISION_EVENTS.CHECKING_REDUNDANCY, {
          personaId: this.id,
          personaName: this.displayName,
          roomId: originalMessage.roomId,
          messageId: originalMessage.id,
          isHumanMessage: originalMessage.senderType === 'human',
          timestamp: Date.now(),
          responseLength: aiResponse.text.trim().length
        } as AICheckingRedundancyEventData);
      }

      const isRedundant = await this.isResponseRedundant(
        aiResponse.text.trim(),
        originalMessage.roomId,
        fullRAGContext.conversationHistory
      );
      */

      if (isRedundant) {
        console.log(`⚠️ ${this.displayName}: [PHASE 3.4] Response marked as REDUNDANT, discarding`);

        // Emit DECIDED_SILENT event to clear AI status indicator
        if (this.client) {
          (this.client.events as unknown as ScopedEventsInterface).room(originalMessage.roomId).emit(AI_DECISION_EVENTS.DECIDED_SILENT, {
            personaId: this.id,
            personaName: this.displayName,
            roomId: originalMessage.roomId,
            messageId: originalMessage.id,
            isHumanMessage: originalMessage.senderType === 'human',
            timestamp: Date.now(),
            confidence: 0.5,
            reason: 'Response was redundant with previous answers',
            gatingModel: 'redundancy-check'
          } as AIDecidedSilentEventData);
        }

        return; // Discard response
      }
      console.log(`✅ ${this.displayName}: [PHASE 3.4] Response not redundant, proceeding to post`);

      // 🔧 SUB-PHASE 3.5: Create and post response
      console.log(`🔧 ${this.displayName}: [PHASE 3.5] Creating response message entity...`);
      const responseMessage = new ChatMessageEntity();
      responseMessage.roomId = originalMessage.roomId;
      responseMessage.senderId = this.id;
      responseMessage.senderName = this.displayName;
      responseMessage.senderType = this.entity.type; // Denormalize from UserEntity (persona)
      responseMessage.content = { text: aiResponse.text.trim(), attachments: [] };
      responseMessage.status = 'sent';
      responseMessage.priority = 'normal';
      responseMessage.timestamp = new Date();
      responseMessage.reactions = [];

      // ✅ Post response via JTAGClient - universal Commands API
      // Prefer this.client if available (set by UserDaemon), fallback to shared instance
      const result = this.client
        ? await this.client.daemons.commands.execute<DataCreateParams<ChatMessageEntity>, DataCreateResult<ChatMessageEntity>>('data/create', {
            context: this.client.context,
            sessionId: this.client.sessionId,
            collection: ChatMessageEntity.collection,
            backend: 'server',
            data: responseMessage
          })
        : await Commands.execute<DataCreateParams<ChatMessageEntity>, DataCreateResult<ChatMessageEntity>>(DATA_COMMANDS.CREATE, {
            collection: ChatMessageEntity.collection,
            backend: 'server',
            data: responseMessage
          });
      console.log(`✅ ${this.displayName}: [PHASE 3.5] Message posted successfully (ID: ${result.data?.id})`);

      if (!result.success) {
        throw new Error(`Failed to create message: ${result.error}`);
      }

      // ✅ Log successful response posting
      AIDecisionLogger.logResponse(
        this.displayName,
        originalMessage.roomId,
        aiResponse.text.trim()
      );

      // Emit POSTED event
      if (this.client && result.data) {
        (this.client.events as unknown as ScopedEventsInterface).room(originalMessage.roomId).emit(AI_DECISION_EVENTS.POSTED, {
          personaId: this.id,
          personaName: this.displayName,
          roomId: originalMessage.roomId,
          messageId: originalMessage.id,
          isHumanMessage: originalMessage.senderType === 'human',
          timestamp: Date.now(),
          responseMessageId: result.data.id,
          passedRedundancyCheck: !isRedundant
        } as AIPostedEventData);
      }

    } catch (error) {
      // Fail silently - real people don't send canned error messages, they just stay quiet
      AIDecisionLogger.logError(
        this.displayName,
        'Response generation/posting',
        error instanceof Error ? error.message : String(error)
      );

      // Emit ERROR event
      if (this.client) {
        (this.client.events as unknown as ScopedEventsInterface).room(originalMessage.roomId).emit(AI_DECISION_EVENTS.ERROR, {
          personaId: this.id,
          personaName: this.displayName,
          roomId: originalMessage.roomId,
          messageId: originalMessage.id,
          isHumanMessage: originalMessage.senderType === 'human',
          timestamp: Date.now(),
          error: error instanceof Error ? error.message : String(error),
          phase: 'generating'
        } as AIErrorEventData);
      }
    }
  }

  /**
   * Handle room update event
   * Updates membership tracking when this persona is added/removed from a room
   */
  private async handleRoomUpdate(roomEntity: RoomEntity): Promise<void> {
    const isMember = roomEntity.members.some((m: { userId: UUID }) => m.userId === this.id);
    const wasInRoom = this.myRoomIds.has(roomEntity.id);

    if (isMember && !wasInRoom) {
      // Added to room
      this.myRoomIds.add(roomEntity.id);
    } else if (!isMember && wasInRoom) {
      // Removed from room
      this.myRoomIds.delete(roomEntity.id);
    }
  }

  /**
   * Check if this persona is mentioned in a message
   * Supports @username mentions and channel directives
   *
   * TODO Phase 2: Use dedicated mention/directive events instead of text parsing
   */
  private isPersonaMentioned(messageText: string): boolean {
    const messageTextLower = messageText.toLowerCase();
    const displayNameLower = this.displayName.toLowerCase();
    const uniqueIdLower = this.entity.uniqueId?.toLowerCase() || '';

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
   * Use fast bag-of-words scoring to decide whether to respond to a message
   *
   * Replaces slow LLM gating (<1ms vs ~500ms+) with deterministic scoring
   * Uses ai/should-respond-fast command for consistent, testable gating
   */
  private async shouldRespondToMessage(
    messageEntity: ChatMessageEntity,
    senderIsHuman: boolean,
    isMentioned: boolean
  ): Promise<boolean> {
    // Rule 1: Always respond if @mentioned (highest priority - forced response)
    if (isMentioned) {
      return true;
    }

    try {
      // Get persona domain keywords (later will come from PersonaEntity config)
      const domainKeywords = this.getPersonaDomainKeywords();

      // Call ai/should-respond-fast command with proper typing
      const result: ShouldRespondFastResult = this.client
        ? await this.client.daemons.commands.execute<ShouldRespondFastParams, ShouldRespondFastResult>('ai/should-respond-fast', {
            context: this.client.context,
            sessionId: this.client.sessionId,
            personaId: this.id,
            contextId: messageEntity.roomId,
            messageId: messageEntity.id,
            senderId: messageEntity.senderId,
            senderName: messageEntity.senderName,
            messageText: messageEntity.content?.text ?? '',
            config: {
              personaName: this.displayName,
              domainKeywords,
              responseThreshold: 50, // Require 50+ points to respond
              alwaysRespondToMentions: true,
              cooldownSeconds: this.minSecondsBetweenResponses
            }
          })
        : await Commands.execute<ShouldRespondFastParams, ShouldRespondFastResult>('ai/should-respond-fast', {
            personaId: this.id,
            contextId: messageEntity.roomId,
            messageId: messageEntity.id,
            senderId: messageEntity.senderId,
            senderName: messageEntity.senderName,
            messageText: messageEntity.content?.text ?? '',
            config: {
              personaName: this.displayName,
              domainKeywords,
              responseThreshold: 50,
              alwaysRespondToMentions: true,
              cooldownSeconds: this.minSecondsBetweenResponses
            }
          });

      if (!result.success) {
        throw new Error(result.error ?? 'Fast gating failed');
      }

      return result.shouldRespond;

    } catch (error) {
      console.error(`❌ ${this.displayName}: Fast gating failed, falling back to heuristics:`, error);

      // Fallback to simple heuristics if command fails
      const heuristics = await this.calculateResponseHeuristics(messageEntity);
      let score = 0;
      if (heuristics.containsQuestion) score += 40;
      if (heuristics.conversationTemp === 'HOT') score += 30;
      if (heuristics.myParticipationRatio < 0.3) score += 20;

      return score >= 50;
    }
  }

  /**
   * Get domain keywords for this persona
   * Reads from UserEntity.personaConfig if available, otherwise infers from name
   */
  private getPersonaDomainKeywords(): string[] {
    // Read from entity configuration if available
    if (this.entity?.personaConfig?.domainKeywords?.length) {
      return [...this.entity.personaConfig.domainKeywords];
    }

    // Fallback: infer from persona name (temporary until all personas configured)
    const nameLower = this.displayName.toLowerCase();

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
    const myMessages = messages.filter(m => m.senderId === this.id);
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
      lastMessage?.senderId !== this.id &&
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
    if (!this.client) {
      console.warn(`⚠️  PersonaUser ${this.displayName}: Cannot check sender type - no client, BLOCKING response`);
      return false; // Fail CLOSED - don't respond if we can't verify (prevents startup loops)
    }

    try {
      // Query the sender's UserEntity to check their type
      const result = await this.client.daemons.commands.execute<DataReadParams, DataReadResult<UserEntity>>('data/read', {
        collection: COLLECTIONS.USERS,
        id: senderId,
        context: this.client.context,
        sessionId: this.client.sessionId,
        backend: 'server'
      });

      if (!result.success || !result.found || !result.data) {
        console.warn(`⚠️  PersonaUser ${this.displayName}: Could not read sender ${senderId}, BLOCKING response`);
        return false; // Fail CLOSED - don't respond if database fails (prevents loops)
      }

      const senderType = result.data.type;
      return senderType === 'human';

    } catch (error) {
      console.error(`❌ PersonaUser ${this.displayName}: Error checking sender type, BLOCKING response:`, error);
      return false; // Fail CLOSED on error (prevents loops)
    }
  }

  /**
   * Get persona database path
   */
  getPersonaDatabasePath(): string {
    return `.continuum/personas/${this.entity.id}/state.sqlite`;
  }

  /**
   * Get genome for this persona (Phase 1.2)
   * Loads the genome entity from database if genomeId is set
   * Returns null if no genome is assigned
   */
  async getGenome(): Promise<GenomeEntity | null> {
    if (!this.entity.genomeId) {
      return null;
    }

    if (!this.client) {
      console.warn(`⚠️  PersonaUser ${this.displayName}: Cannot load genome - no client`);
      return null;
    }

    try {
      const result = await this.client.daemons.commands.execute<DataReadParams, DataReadResult<GenomeEntity>>('data/read', {
        collection: 'genomes',
        id: this.entity.genomeId,
        context: this.client.context,
        sessionId: this.client.sessionId,
        backend: 'server'
      });

      if (!result.success || !result.found || !result.data) {
        console.warn(`⚠️  PersonaUser ${this.displayName}: Genome ${this.entity.genomeId} not found`);
        return null;
      }

      return result.data;

    } catch (error) {
      console.error(`❌ PersonaUser ${this.displayName}: Error loading genome:`, error);
      return null;
    }
  }

  /**
   * Set genome for this persona (Phase 1.2)
   * Updates the genomeId field and persists to database
   */
  async setGenome(genomeId: UUID): Promise<boolean> {
    if (!this.client) {
      console.warn(`⚠️  PersonaUser ${this.displayName}: Cannot set genome - no client`);
      return false;
    }

    try {
      // Update entity
      this.entity.genomeId = genomeId;

      // Persist to database
      const result = await this.client.daemons.commands.execute<DataUpdateParams<UserEntity>, DataUpdateResult<UserEntity>>('data/update', {
        collection: COLLECTIONS.USERS,
        id: this.entity.id,
        data: { genomeId },
        context: this.client.context,
        sessionId: this.client.sessionId,
        backend: 'server'
      });

      if (!result.success) {
        console.error(`❌ PersonaUser ${this.displayName}: Failed to update genome: ${result.error}`);
        return false;
      }

      return true;

    } catch (error) {
      console.error(`❌ PersonaUser ${this.displayName}: Error setting genome:`, error);
      return false;
    }
  }

  /**
   * RAG Context Storage - Store conversation context for a room
   * Enables persona to maintain context across sessions
   *
   * Phase 2: Direct ArtifactsDaemon access (proper implementation pending)
   * For now, store in memory until artifact commands are implemented
   */
  async storeRAGContext(roomId: UUID, context: PersonaRAGContext): Promise<void> {
    if (!this.client) {
      console.warn(`⚠️  PersonaUser ${this.displayName}: Cannot store RAG context - no client`);
      return;
    }

    // TODO Phase 2: Use artifacts daemon when commands are implemented
    // await this.client.daemons.artifacts.writeJSON(...)
  }

  /**
   * RAG Context Loading - Load conversation context for a room
   * Returns null if no context exists yet
   *
   * Phase 2: Direct ArtifactsDaemon access (proper implementation pending)
   * For now, return null until artifact commands are implemented
   */
  async loadRAGContext(roomId: UUID): Promise<PersonaRAGContext | null> {
    if (!this.client) {
      console.warn(`⚠️  PersonaUser ${this.displayName}: Cannot load RAG context - no client`);
      return null;
    }

    // TODO Phase 2: Use artifacts daemon when commands are implemented
    // return await this.client.daemons.artifacts.readJSON<PersonaRAGContext>(...)
    return null;
  }

  /**
   * Update RAG Context - Add new message to context and trim if needed
   */
  async updateRAGContext(roomId: UUID, message: ChatMessageEntity): Promise<void> {
    // Load existing context or create new
    let context = await this.loadRAGContext(roomId);
    if (!context) {
      context = {
        roomId,
        personaId: this.id,
        messages: [],
        lastUpdated: new Date().toISOString(),
        tokenCount: 0
      };
    }

    // Add new message to context
    context.messages.push({
      senderId: message.senderId,
      senderName: message.senderName,
      text: message.content?.text || '',
      timestamp: typeof message.timestamp === 'string' ? message.timestamp : message.timestamp.toISOString()
    });

    // Keep only last 50 messages (simple context window for now)
    if (context.messages.length > 50) {
      context.messages = context.messages.slice(-50);
    }

    context.lastUpdated = new Date().toISOString();

    // Store updated context
    await this.storeRAGContext(roomId, context);
  }

  /**
   * PersonaUser creation recipe
   *
   * ARCHITECTURE NOTE: Creation still uses DataDaemon for now
   * - DataDaemon is the internal data layer (system-level operations)
   * - Commands.execute() is for user-level operations (PersonaUser responding to chat)
   * - This maintains proper abstraction: creation is system concern, chat is user concern
   *
   * Recipe steps:
   * 1. Create UserEntity in database
   * 2. Create UserStateEntity in database with persona defaults
   * 3. Add to rooms if specified
   * 4. Return PersonaUser instance (UserDaemon will create persistent instance)
   */
  static async create(
    params: UserCreateParams,
    _context: JTAGContext,
    _router: JTAGRouter
  ): Promise<PersonaUser> {
    // STEP 1: Create UserEntity in database
    const userEntity = new UserEntity();
    userEntity.type = 'persona';
    userEntity.uniqueId = params.uniqueId;
    userEntity.displayName = params.displayName;
    userEntity.status = params.status ?? 'online';
    userEntity.lastActiveAt = new Date();
    userEntity.capabilities = params.capabilities ?? getDefaultCapabilitiesForType('persona');
    userEntity.sessionsActive = [];
    // Optional extended fields for personas
    if (params.modelConfig) {
      Object.assign(userEntity, { modelConfig: params.modelConfig });
    }
    // createdAt, updatedAt, version, id handled by constructor

    const storedEntity = await DataDaemon.store<UserEntity>(
      COLLECTIONS.USERS,
      userEntity
    );

    // STEP 2: Create UserStateEntity with persona-specific defaults
    const userState = this.getDefaultState(storedEntity.id);
    userState.preferences = getDefaultPreferencesForType('persona');

    const storedState = await DataDaemon.store<UserStateEntity>(
      COLLECTIONS.USER_STATES,
      userState
    );

    // STEP 3: Auto-join "general" room (all users start here)
    try {
      await this.addToGeneralRoom(storedEntity.id, params.displayName);
    } catch (error) {
      console.error(`❌ PersonaUser.create: Failed to add to general room:`, error);
    }

    // STEP 4: Add persona to additional rooms if specified
    if (params.addToRooms && params.addToRooms.length > 0) {
      for (const roomId of params.addToRooms) {
        await this.addToRoom(storedEntity.id, roomId, params.displayName);
      }
    }

    // STEP 5: Create PersonaUser instance (client injected by UserDaemon)
    const storage = new MemoryStateBackend();
    return new PersonaUser(storedEntity, storedState, storage, undefined);
  }

  /**
   * === THOUGHT STREAM COORDINATION METHODS ===
   * RTOS-inspired: Signal, mutex, semaphore, condition variable primitives
   */

  /**
   * Evaluate whether to respond (delegates to ai/should-respond command)
   *
   * Returns the command's shouldRespond boolean directly - no threshold logic here!
   * The command handles all gating logic internally.
   */
  private async evaluateShouldRespond(
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
  }> {

    try {
      // FAST-PATH: If directly mentioned by name, always respond (skip expensive LLM call)
      if (isMentioned) {
        return {
          shouldRespond: true,
          confidence: 1.0,
          reason: 'Directly mentioned by name',
          model: 'fast-path'
        };
      }

      // Build RAG context for gating decision (recent messages only, max 5 minutes old)
      // Include recent context BUT filter out old messages from different conversation windows
      const ragBuilder = new ChatRAGBuilder();
      const ragContext = await ragBuilder.buildContext(
        message.roomId,
        this.id,
        {
          maxMessages: 30,  // Fetch more messages since we filter heavily (system messages + 5min window)
          maxMemories: 0,
          includeArtifacts: false,
          includeMemories: false,
          currentMessage: {
            role: 'user',
            content: message.content.text,
            name: message.senderName,
            timestamp: this.timestampToNumber(message.timestamp)
          }
        }
      );

      // FIX 1: Configurable time window per persona (default 30 minutes instead of 5)
      // Smarter models might not need as much context, smaller models need more
      const contextWindowMinutes = this.entity?.personaConfig?.contextWindowMinutes ?? 30;
      const contextWindowMs = contextWindowMinutes * 60 * 1000;
      const cutoffTime = Date.now() - contextWindowMs;

      // FIX 2: Configurable minimum messages per persona
      // Always include at least N messages for context, regardless of time window
      const minContextMessages = this.entity?.personaConfig?.minContextMessages ?? 3;

      // Filter conversation history to only include REAL messages (not system/welcome)
      const nonSystemMessages = ragContext.conversationHistory.filter(msg => {
        // Exclude system messages (welcome, announcements) from gating context
        // These confuse the AI into thinking there's an active conversation
        const isSystemMessage = msg.role === 'system' ||
                                msg.name === 'System' ||
                                msg.content.startsWith('Welcome to') ||
                                msg.content.includes('I\'m Claude Code');

        return !isSystemMessage;
      });

      // Apply time window filter
      const timeFilteredHistory = nonSystemMessages.filter(msg => {
        const msgTime = msg.timestamp ?? 0;
        return msgTime >= cutoffTime;
      });

      // FIX 3: Ensure minimum messages regardless of time window
      // If time filtering removed too many messages, include older ones to meet minimum
      let recentHistory = timeFilteredHistory;
      if (recentHistory.length < minContextMessages && nonSystemMessages.length >= minContextMessages) {
        recentHistory = nonSystemMessages.slice(-minContextMessages);
        console.log(`⚠️ ${this.displayName}: Time window too restrictive (${recentHistory.length}/${minContextMessages}), using last ${minContextMessages} messages`);
      }

      // Use filtered context for gating decision
      const filteredRagContext = {
        ...ragContext,
        conversationHistory: recentHistory
      };

      // Get gating model from persona config (defaults to llama3.2:3b for reliability)
      const gatingModelMap = {
        'deterministic': null,  // Use bag-of-words (not implemented yet)
        'small': 'llama3.2:1b',  // Fast but unreliable JSON parsing (~150-200ms)
        'full': 'llama3.2:3b'   // More accurate and reliable JSON (~400-500ms)
      };
      const gatingModelKey = this.entity?.personaConfig?.gatingModel ?? 'full'; // Changed default from 'small' to 'full'
      const gatingModel = gatingModelMap[gatingModelKey] ?? 'llama3.2:3b';

      // Use AIDecisionService for centralized AI logic
      const decisionContext: AIDecisionContext = {
        personaId: this.id,
        personaName: this.displayName,
        roomId: message.roomId,
        triggerMessage: message,
        ragContext: filteredRagContext
      };

      const result = await AIDecisionService.evaluateGating(decisionContext, {
        model: gatingModel,
        temperature: 0.3
      });

      // Return with RAG context summary for logging
      return {
        shouldRespond: result.shouldRespond,
        confidence: result.confidence,
        reason: result.reason,
        model: result.model,
        ragContextSummary: {
          totalMessages: ragContext.conversationHistory.length,
          filteredMessages: recentHistory.length,
          timeWindowMinutes: contextWindowMinutes
        },
        conversationHistory: recentHistory.map(msg => ({
          name: msg.name ?? 'Unknown',
          content: msg.content,
          timestamp: msg.timestamp
        }))
      };

    } catch (error) {
      console.error(`❌ ${this.displayName}: Should-respond evaluation failed:`, error);
      return {
        shouldRespond: isMentioned,
        confidence: isMentioned ? 1.0 : 0.5,
        reason: 'Error in evaluation',
        model: 'error'
      };
    }
  }

  /**
   * Broadcast thought to stream (SIGNAL primitive)
   */
  private async broadcastThought(messageId: string, thought: Thought): Promise<void> {
    try {
      const coordinator = getThoughtStreamCoordinator();
      await coordinator.broadcastThought(messageId, thought);
    } catch (error) {
      console.error(`❌ ${this.displayName}: Failed to broadcast thought (non-fatal):`, error);
      // Non-fatal: continue without coordination
    }
  }

}
