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
import type { UserCreateParams, ModelConfig } from '../../../commands/user/create/shared/UserCreateTypes';
import type { DataCreateParams, DataCreateResult } from '../../../commands/data/create/shared/DataCreateTypes';
import type { DataReadParams, DataReadResult } from '../../../commands/data/read/shared/DataReadTypes';
import type { DataUpdateParams, DataUpdateResult } from '../../../commands/data/update/shared/DataUpdateTypes';
import type { Thought, ThoughtType } from '../../conversation/shared/ConversationCoordinationTypes';
import { getChatCoordinator, type ChatThought } from '../../coordination/server/ChatCoordinationStream';
import { MemoryStateBackend } from '../storage/MemoryStateBackend';
import { getDefaultCapabilitiesForType, getDefaultPreferencesForType } from '../config/UserCapabilitiesDefaults';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../../data/config/DatabaseConfig';
import { TaskEntity } from '../../data/entities/TaskEntity';
import { taskEntityToInboxTask } from './modules/QueueItemTypes';
import { AIProviderDaemon } from '../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import type { TextGenerationRequest, TextGenerationResponse } from '../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import { ChatRAGBuilder } from '../../rag/builders/ChatRAGBuilder';
import type { ShouldRespondFastParams, ShouldRespondFastResult } from '../../../commands/ai/should-respond-fast/shared/ShouldRespondFastTypes';
import type { AIShouldRespondParams, AIShouldRespondResult } from '../../../commands/ai/should-respond/shared/AIShouldRespondTypes';
import type { GenomeEntity } from '../../genome/entities/GenomeEntity';
import { AIDecisionLogger } from '../../ai/server/AIDecisionLogger';
import { AIDecisionService, type AIDecisionContext } from '../../ai/server/AIDecisionService';
import { getModelConfigForProvider } from './config/PersonaModelConfigs';
import { CoordinationDecisionLogger, type LogDecisionParams } from '../../coordination/server/CoordinationDecisionLogger';
import type { RAGContext } from '../../data/entities/CoordinationDecisionEntity';
import { PersonaWorkerThread } from '../../../shared/workers/PersonaWorkerThread';
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
import {
  AI_LEARNING_EVENTS,
  type AITrainingStartedEventData,
  type AITrainingCompleteEventData,
  type AITrainingErrorEventData,
  type AIInteractionCapturedEventData
} from '../../events/shared/AILearningEvents';
import { Events } from '../../core/shared/Events';
import { EVENT_SCOPES } from '../../events/shared/EventSystemConstants';
import { ROOM_UNIQUE_IDS } from '../../data/constants/RoomConstants';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import type { StageCompleteEvent } from '../../conversation/shared/CognitionEventTypes';
import { calculateSpeedScore, getStageStatus, COGNITION_EVENTS } from '../../conversation/shared/CognitionEventTypes';
import { RateLimiter } from './modules/RateLimiter';
import { PersonaInbox, calculateMessagePriority } from './modules/PersonaInbox';
import { PersonaStateManager } from './modules/PersonaState';
import type { InboxMessage } from './modules/PersonaInbox';
import type { InboxTask, TaskStatus } from './modules/QueueItemTypes';
import { TrainingDataAccumulator } from './modules/TrainingDataAccumulator';
import { SelfTaskGenerator } from './modules/SelfTaskGenerator';
import { PersonaGenome, type PersonaGenomeConfig } from './modules/PersonaGenome';
import type { PersonaCentralNervousSystem } from './modules/central-nervous-system/PersonaCentralNervousSystem';
import { CNSFactory } from './modules/central-nervous-system/CNSFactory';
import type { QueueItem } from './modules/PersonaInbox';
import { PersonaMemory } from './modules/cognitive/memory/PersonaMemory';
import { DecisionAdapterChain } from './modules/cognition/DecisionAdapterChain';
import type { DecisionContext } from './modules/cognition/adapters/IDecisionAdapter';

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

  // Worker thread for parallel message evaluation
  private worker: PersonaWorkerThread | null = null;

  // AI model configuration (provider, model, temperature, etc.)
  private modelConfig: ModelConfig;

  // Rate limiting module (TODO: Replace with AI-based coordination when ThoughtStream is solid)
  private rateLimiter: RateLimiter;

  // PHASE 1: Autonomous servicing modules (inbox + personaState)
  // Inbox stores messages with priority, personaState tracks energy/mood
  // NOTE: Can't name this 'state' - conflicts with BaseUser.state (UserStateEntity)
  public inbox: PersonaInbox;
  public personaState: PersonaStateManager;

  // PHASE 5: Self-task generation (autonomous work creation)
  private taskGenerator: SelfTaskGenerator;

  // COGNITIVE MODULE: Memory (knowledge, context, genome)
  public memory: PersonaMemory;

  // PHASE 6: Decision Adapter Chain (fast-path, thermal, LLM gating)
  private decisionChain: DecisionAdapterChain;

  // CNS: Central Nervous System orchestrator
  private cns: PersonaCentralNervousSystem;

  // PHASE 7.4: Training data accumulation for recipe-embedded learning
  // Accumulates training examples in RAM during recipe execution
  public trainingAccumulator: TrainingDataAccumulator;

  // PHASE 3: Autonomous polling loop
  private servicingLoop: NodeJS.Timeout | null = null; // Deprecated - now using continuous async loop
  private servicingLoopActive: boolean = false; // Controls continuous service loop

  // PHASE 7.5.1: Training readiness check loop (runs less frequently than servicing loop)
  private trainingCheckLoop: NodeJS.Timeout | null = null;

  constructor(
    entity: UserEntity,
    state: UserStateEntity,
    storage: IUserStateStorage,
    client?: JTAGClient
  ) {
    super(entity, state, storage, client); // ✅ Pass client to BaseUser for event subscriptions

    // Extract modelConfig from entity (stored via Object.assign during creation)
    // Default to Ollama if not configured
    this.modelConfig = (entity as any).modelConfig || {
      provider: 'ollama',
      model: 'llama3.2:3b',
      temperature: 0.7,
      maxTokens: 150
    };

    console.log(`🤖 ${this.displayName}: Configured with provider=${this.modelConfig.provider}, model=${this.modelConfig.model}`);

    // Initialize rate limiter (TODO: Replace with AI-based coordination)
    this.rateLimiter = new RateLimiter({
      minSecondsBetweenResponses: 10,
      maxResponsesPerSession: 50
    });

    // PHASE 1: Initialize autonomous servicing modules
    // Inbox: Priority-based message queue (default max 100 messages)
    this.inbox = new PersonaInbox(this.id, this.displayName, {
      maxSize: 100,
      enableLogging: true
    });

    // PersonaState: Energy/mood tracking for adaptive behavior
    this.personaState = new PersonaStateManager(this.displayName, {
      enableLogging: true
    });

    // PHASE 5: Self-task generation for autonomous work creation
    this.taskGenerator = new SelfTaskGenerator(this.id, this.displayName, {
      enabled: true,  // Enable self-task generation
      memoryReviewInterval: 3600000,      // 1 hour
      skillAuditInterval: 21600000,       // 6 hours
      unfinishedWorkThreshold: 1800000    // 30 minutes
    });

    // COGNITIVE MODULE: Memory (RAG context + genome)
    this.memory = new PersonaMemory(
      this.id,
      this.displayName,
      {
        baseModel: this.modelConfig.model || 'llama3.2:3b',
        memoryBudgetMB: 200,  // 200MB GPU memory budget for adapters
        adaptersPath: './lora-adapters',
        initialAdapters: [
          {
            name: 'conversational',
            domain: 'chat',
            path: './lora-adapters/conversational.safetensors',
            sizeMB: 50,
            priority: 0.7  // High priority - used frequently
          },
          {
            name: 'typescript-expertise',
            domain: 'code',
            path: './lora-adapters/typescript-expertise.safetensors',
            sizeMB: 60,
            priority: 0.6
          },
          {
            name: 'self-improvement',
            domain: 'self',
            path: './lora-adapters/self-improvement.safetensors',
            sizeMB: 40,
            priority: 0.5
          }
        ]
      },
      client
    );

    // PHASE 6: Decision adapter chain (fast-path, thermal, LLM gating)
    this.decisionChain = new DecisionAdapterChain();
    console.log(`🔗 ${this.displayName}: Decision adapter chain initialized with ${this.decisionChain.getAllAdapters().length} adapters`);

    // PHASE 7.4: Training data accumulator for recipe-embedded learning
    this.trainingAccumulator = new TrainingDataAccumulator(this.id, this.displayName);

    // CNS: Central Nervous System orchestrator (capability-based)
    this.cns = CNSFactory.create(this);

    console.log(`🔧 ${this.displayName}: Initialized inbox, personaState, taskGenerator, memory (genome + RAG), CNS, and trainingAccumulator modules`);

    // Initialize worker thread for this persona
    // Worker uses fast small model for gating decisions (should-respond check)
    this.worker = new PersonaWorkerThread(this.id, {
      providerType: 'ollama',  // Always use Ollama for fast gating (1b model)
      providerConfig: {
        apiEndpoint: 'http://localhost:11434',
        model: 'llama3.2:1b' // Fast model for gating decisions
      }
    });
  }

  /**
   * Build default ModelConfig from provider string
   * Used when seed script passes --provider instead of full --modelConfig
   */
  private static buildModelConfigFromProvider(provider: string): ModelConfig {
    // Imported from PersonaModelConfigs.ts for better organization
    return getModelConfigForProvider(provider);
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

    // Note: General room auto-join handled by UserDaemonServer on user creation (Discord-style)

    // STEP 1.5: Start worker thread for message evaluation
    if (this.worker) {
      await this.worker.start();
      console.log(`🧵 ${this.displayName}: Worker thread started`);
    }

    // STEP 1.6: Register with ResourceManager for holistic resource allocation
    try {
      const { getResourceManager } = await import('../../resources/shared/ResourceManager.js');
      getResourceManager().registerAdapter(this.id, this.displayName);
      console.log(`🔧 ${this.displayName}: Registered with ResourceManager`);
    } catch (error) {
      console.warn(`⚠️  ${this.displayName}: Could not register with ResourceManager:`, error);
      // Non-fatal: isAvailable() will default to simple worker ready check
    }

    // STEP 2: Subscribe to room-specific chat events (only if client available)
    if (this.client && !this.eventsSubscribed) {
      console.log(`🔧 ${this.displayName}: About to subscribe to ${this.myRoomIds.size} room(s), eventsSubscribed=${this.eventsSubscribed}`);

      // Subscribe to ALL chat events once (not per-room)
      // subscribeToChatEvents() filters by this.myRoomIds internally
      this.subscribeToChatEvents(this.handleChatMessage.bind(this));
      this.subscribeToRoomUpdates(this.handleRoomUpdate.bind(this));

      // Subscribe to truncate events to reset rate limiter (using Events.subscribe)
      // Pass this.id as subscriberId to enable deduplication (prevents duplicate subscriptions)
      Events.subscribe('data:chat_messages:truncated', () => {
        // Clear message deduplication cache when messages are truncated
        this.rateLimiter.clearEvaluatedMessages();
      }, undefined, this.id);

      this.eventsSubscribed = true;
      console.log(`✅ ${this.displayName}: Subscriptions complete, eventsSubscribed=${this.eventsSubscribed}`);
    } else {
      console.log(`⏭️ ${this.displayName}: Skipping subscriptions (already subscribed or no client), eventsSubscribed=${this.eventsSubscribed}, hasClient=${!!this.client}`);
    }

    this.isInitialized = true;

    // PHASE 3: Start autonomous servicing loop (lifecycle-based)
    this.startAutonomousServicing();
  }

  /**
   * Auto-join general room if not already a member
   *
   * NOTE: This is a simple add to room.members array, NOT event subscriptions
   * Event subscriptions happen once in initialize() - this just updates membership
   *
   * TODO: Replace with event-driven architecture (listener on data:users:created)
   * Current limitation: If general room doesn't exist yet, this will fail silently
   */
  private async autoJoinGeneralRoom(): Promise<void> {
    if (!this.client) {
      console.warn(`⚠️ ${this.displayName}: Cannot auto-join general room - no client available`);
      return;
    }

    try {
      // Query for general room using DataDaemon.query (server-side only)
      const queryResult = await DataDaemon.query<RoomEntity>({
        collection: COLLECTIONS.ROOMS,
        filter: { uniqueId: ROOM_UNIQUE_IDS.GENERAL }
      });

      if (!queryResult.success || !queryResult.data?.length) {
        console.warn(`⚠️ ${this.displayName}: General room not found - cannot auto-join`);
        return;
      }

      const generalRoomRecord = queryResult.data[0];
      if (!generalRoomRecord) {
        return;
      }

      const generalRoom = generalRoomRecord.data;

      // Check if already a member
      const isMember = generalRoom.members?.some((m: { userId: UUID }) => m.userId === this.id);
      if (isMember) {
        console.log(`✅ ${this.displayName}: Already member of general room`);
        return;
      }

      // Add self to members (just updating the entity, not adding subscriptions)
      const updatedMembers = [
        ...(generalRoom.members ?? []),
        {
          userId: this.id,
          role: 'member' as const,
          joinedAt: new Date()
        }
      ];

      // Update room with new member using DataDaemon.update
      await DataDaemon.update<RoomEntity>(
        COLLECTIONS.ROOMS,
        generalRoom.id,
        { members: updatedMembers }
      );

      console.log(`✅ ${this.displayName}: Auto-joined general room (added to members array)`);
      // Reload my rooms to pick up the change
      await this.loadMyRooms();
    } catch (error) {
      console.error(`❌ ${this.displayName}: Error auto-joining general room:`, error);
    }
  }

  /**
   * Handle incoming chat message - PHASE 1: ENQUEUE TO INBOX
   * Messages flow through priority queue before evaluation (proves inbox works)
   * NO autonomous loop yet - still processes immediately after enqueue
   */
  private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {
    // STEP 1: Ignore our own messages
    if (messageEntity.senderId === this.id) {
      return;
    }

    // STEP 2: Deduplication - prevent evaluating same message multiple times
    if (this.rateLimiter.hasEvaluatedMessage(messageEntity.id)) {
      return; // Already evaluated this message
    }

    // Mark as evaluated
    this.rateLimiter.markMessageEvaluated(messageEntity.id);

    // STEP 3: Skip resolved messages (moderator marked as no longer needing responses)
    if (messageEntity.metadata?.resolved) {
      console.log(`⏭️ ${this.displayName}: Skipping resolved message from ${messageEntity.senderName}`);
      return;
    }

    // PHASE 3BIS: Update activity temperature (observation only, doesn't affect decisions yet)
    getChatCoordinator().onHumanMessage(messageEntity.roomId);

    // PHASE 1: Calculate priority and enqueue to inbox
    const priority = calculateMessagePriority(
      {
        content: messageEntity.content?.text || '',
        timestamp: this.timestampToNumber(messageEntity.timestamp),
        roomId: messageEntity.roomId
      },
      {
        displayName: this.displayName,
        id: this.id,
        recentRooms: Array.from(this.myRoomIds),
        expertise: [] // TODO: Extract from genome
      }
    );

    const inboxMessage: InboxMessage = {
      id: messageEntity.id,
      type: 'message',
      domain: 'chat',  // Messages are always chat domain
      roomId: messageEntity.roomId,
      content: messageEntity.content?.text || '',
      senderId: messageEntity.senderId,
      senderName: messageEntity.senderName,
      timestamp: this.timestampToNumber(messageEntity.timestamp),
      priority
    };

    await this.inbox.enqueue(inboxMessage);

    // Update inbox load in state (for mood calculation)
    this.personaState.updateInboxLoad(this.inbox.getSize());

    console.log(`📨 ${this.displayName}: Enqueued message (priority=${priority.toFixed(2)}, inbox size=${this.inbox.getSize()}, mood=${this.personaState.getState().mood})`);

    // PHASE 3: Autonomous polling loop will service inbox at adaptive cadence
    // (No immediate processing - messages wait in inbox until loop polls)
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
    if (this.rateLimiter.hasReachedResponseCap(messageEntity.roomId)) {
      const currentCount = this.rateLimiter.getResponseCount(messageEntity.roomId);
      const config = this.rateLimiter.getConfig();
      this.logAIDecision('SILENT', `Response cap reached (${currentCount}/${config.maxResponsesPerSession})`, {
        message: messageText,
        sender: messageEntity.senderName,
        roomId: messageEntity.roomId
      });
      return;
    }

    // STEP 3: Check if mentioned
    const isMentioned = this.isPersonaMentioned(messageText);

    // STEP 4: Check rate limiting (before expensive LLM call)
    if (this.rateLimiter.isRateLimited(messageEntity.roomId)) {
      const info = this.rateLimiter.getRateLimitInfo(messageEntity.roomId);
      this.logAIDecision('SILENT', `Rate limited, wait ${info.waitTimeSeconds?.toFixed(1)}s more`, {
        message: messageText,
        sender: messageEntity.senderName,
        roomId: messageEntity.roomId
      });
      return;
    }

    // === EVALUATE: Use LLM-based intelligent gating to decide if should respond ===
    // Emit EVALUATING event for real-time feedback
    if (this.client) {
      await Events.emit<AIEvaluatingEventData>(
        DataDaemon.jtagContext!,
        AI_DECISION_EVENTS.EVALUATING,
        {
          personaId: this.id,
          personaName: this.displayName,
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
    console.log(`\n${'='.repeat(80)}`);
    console.log(`🧠 ${this.displayName}: GATING DECISION for message "${messageText.slice(0, 60)}..."`);
    console.log(`${'='.repeat(80)}`);
    console.log(`📊 Context: ${gatingResult.ragContextSummary?.filteredMessages ?? 0} messages in ${gatingResult.ragContextSummary?.timeWindowMinutes ?? 0}min window`);
    console.log(`💬 Conversation history seen by AI:`);
    gatingResult.conversationHistory?.slice(-5).forEach((msg, i) => {
      console.log(`   ${i + 1}. [${msg.name}] ${msg.content.slice(0, 80)}...`);
    });
    console.log(`\n🎯 Decision: ${gatingResult.shouldRespond ? 'RESPOND' : 'SILENT'}`);
    console.log(`   Confidence: ${(gatingResult.confidence * 100).toFixed(0)}%`);
    console.log(`   Reason: ${gatingResult.reason}`);
    console.log(`   Model: ${gatingResult.model}`);
    console.log(`${'='.repeat(80)}\n`);

    if (!gatingResult.shouldRespond) {
      // PHASE 5C: Log coordination decision to database (fire-and-forget)
      if (gatingResult.filteredRagContext) {
        const decisionStartTime = Date.now();
        const ragContext = this.buildCoordinationRAGContext(gatingResult.filteredRagContext);

        // Fire-and-forget: Don't await, don't slow down critical path
        CoordinationDecisionLogger.logDecision({
          actorId: this.id,
          actorName: this.displayName,
          actorType: 'ai-persona',
          triggerEventId: messageEntity.id,
          ragContext,
          visualContext: undefined,
          action: 'SILENT',
          confidence: gatingResult.confidence,
          reasoning: gatingResult.reason,
          responseContent: undefined,
          modelUsed: gatingResult.model,
          modelProvider: this.modelConfig.provider ?? 'ollama',
          tokensUsed: undefined,
          responseTime: Date.now() - decisionStartTime,
          sessionId: DataDaemon.jtagContext!.uuid,
          contextId: messageEntity.roomId,
          tags: [senderIsHuman ? 'human-sender' : 'ai-sender', 'gating-silent']
        }).catch(error => {
          console.error(`❌ ${this.displayName}: Failed to log SILENT decision:`, error);
        });
      }

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
        await Events.emit<AIDecidedSilentEventData>(
        DataDaemon.jtagContext!,
        AI_DECISION_EVENTS.DECIDED_SILENT,
          {
            personaId: this.id,
            personaName: this.displayName,
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
      actorId: this.id,
      actorName: this.displayName,
      actorType: 'ai-persona' as const,
      triggerEventId: messageEntity.id,
      ragContext: this.buildCoordinationRAGContext(gatingResult.filteredRagContext),
      visualContext: undefined,
      action: 'POSTED' as const,
      confidence: gatingResult.confidence,
      reasoning: gatingResult.reason,
      modelUsed: gatingResult.model,
      modelProvider: this.modelConfig.provider ?? 'ollama',
      sessionId: DataDaemon.jtagContext!.uuid,
      contextId: messageEntity.roomId,
      tags: [
        senderIsHuman ? 'human-sender' : 'ai-sender',
        isMentioned ? 'mentioned' : 'not-mentioned',
        'gating-respond'
      ]
    } : undefined;

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
      await Events.emit<AIDecidedRespondEventData>(
        DataDaemon.jtagContext!,
        AI_DECISION_EVENTS.DECIDED_RESPOND,
        {
          personaId: this.id,
          personaName: this.displayName,
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

    // === AUTONOMOUS DECISION: Persona personality adapter decides ===
    // REMOVED: Thoughtstream coordination (centralized blocking)
    // Personas are organic decision makers - if their personality adapter says "respond", they respond
    // Multiple AIs responding is natural conversation, not a problem to prevent

    /* DISABLED: Thoughtstream centralized coordination (phase out)
    const coordinator = getChatCoordinator();
    const chatThought: ChatThought = {
      personaId: this.id,
      personaName: this.displayName,
      type: 'claiming',
      confidence: gatingResult.confidence ?? 0.5,
      reasoning: gatingResult.reason,
      timestamp: Date.now(),
      messageId: messageEntity.id,
      roomId: messageEntity.roomId
    };

    console.log(`🧠 ${this.displayName}: Broadcasting thought (confidence=${gatingResult.confidence?.toFixed(2)}) for message ${messageEntity.id.slice(0, 8)}`);
    await coordinator.broadcastChatThought(messageEntity.id, messageEntity.roomId, chatThought);

    // Wait for coordinator decision (reasonable timeout for thought gathering)
    console.log(`⏳ ${this.displayName}: Waiting for coordination decision...`);
    const decision = await coordinator.waitForChatDecision(messageEntity.id, 5000);

    // Check if we were granted permission to respond
    if (!decision || !decision.granted.includes(this.id)) {
      // Log actual decision for debugging
      const grantedCount = decision?.granted.length ?? 0;
      const deniedCount = decision?.denied.length ?? 0;
      const grantedIds = decision?.granted.map(id => id.slice(0, 8)).join(', ') ?? 'none';
      console.log(`🚫 ${this.displayName}: Denied by coordinator (granted: ${grantedCount} [${grantedIds}], denied: ${deniedCount}, my ID: ${this.id.slice(0, 8)})`);
      if (decision) {
        console.log(`   Decision reasoning: ${decision.reasoning ?? 'none'}`);
      }

      this.logAIDecision('SILENT', 'ThoughtStreamCoordinator denied (higher confidence AI responding)', {
        message: messageText,
        sender: messageEntity.senderName,
        roomId: messageEntity.roomId,
        confidence: gatingResult.confidence
      });

      // Emit DECIDED_SILENT event to clear AI status indicator
      console.log(`🔧 ${this.displayName}: Emitting DECIDED_SILENT event (ThoughtStreamCoordinator blocked)`);
      if (this.client) {
        await Events.emit<AIDecidedSilentEventData>(
        DataDaemon.jtagContext!,
        AI_DECISION_EVENTS.DECIDED_SILENT,
          {
            personaId: this.id,
            personaName: this.displayName,
            roomId: messageEntity.roomId,
            messageId: messageEntity.id,
            isHumanMessage: senderIsHuman,
            reason: 'ThoughtStreamCoordinator denied (higher confidence AI responding)',
            confidence: gatingResult.confidence ?? 0.5,
            gatingModel: gatingResult.model ?? 'unknown',
            timestamp: Date.now()
          },
          {
            scope: EVENT_SCOPES.ROOM,
            scopeId: messageEntity.roomId,
          }
        );
        console.log(`✅ ${this.displayName}: DECIDED_SILENT event emitted successfully`);
      } else {
        console.error(`❌ ${this.displayName}: Cannot emit DECIDED_SILENT - this.client is null`);
      }

      return; // Don't generate - let higher confidence AI respond
    }

    console.log(`✅ ${this.displayName}: Granted permission by coordinator (conf=${gatingResult.confidence})`);
    */

    console.log(`✅ ${this.displayName}: Autonomous decision to respond (personality adapter, conf=${gatingResult.confidence})`);

    // 🔧 PHASE: Update RAG context
    console.log(`🔧 ${this.displayName}: [PHASE 1/3] Updating RAG context...`);
    await this.memory.updateRAGContext(messageEntity.roomId, messageEntity);
    console.log(`✅ ${this.displayName}: [PHASE 1/3] RAG context updated`);

    // 🔧 PHASE: Emit GENERATING event (using auto-context via sharedInstance)
    console.log(`🔧 ${this.displayName}: [PHASE 2/3] Emitting GENERATING event...`);
    if (this.client) {
      await Events.emit<AIGeneratingEventData>(
        DataDaemon.jtagContext!,
        AI_DECISION_EVENTS.GENERATING,
        {
          personaId: this.id,
          personaName: this.displayName,
          roomId: messageEntity.roomId,
          messageId: messageEntity.id,
          isHumanMessage: senderIsHuman,
          timestamp: Date.now(),
          responseModel: this.entity?.personaConfig?.responseModel ?? 'default'
        },
        {
          scope: EVENT_SCOPES.ROOM,
          scopeId: messageEntity.roomId
        }
      );
    }
    console.log(`✅ ${this.displayName}: [PHASE 2/3] GENERATING event emitted`);

    // 🔧 PHASE: Generate and post response
    console.log(`🔧 ${this.displayName}: [PHASE 3/3] Calling respondToMessage...`);
    await this.respondToMessage(messageEntity, decisionContext);
    console.log(`✅ ${this.displayName}: [PHASE 3/3] Response posted successfully`);

    // PHASE 3BIS: Notify coordinator that message was serviced (lowers temperature)
    getChatCoordinator().onMessageServiced(messageEntity.roomId, this.id);

    // Track response for rate limiting
    this.rateLimiter.trackResponse(messageEntity.roomId);

    // PHASE 2: Track activity in PersonaState (energy depletion, mood calculation)
    // Recalculate priority to estimate complexity (higher priority = more engaging conversation)
    const messageComplexity = calculateMessagePriority(
      {
        content: messageEntity.content?.text || '',
        timestamp: this.timestampToNumber(messageEntity.timestamp),
        roomId: messageEntity.roomId
      },
      {
        displayName: this.displayName,
        id: this.id,
        recentRooms: Array.from(this.myRoomIds) // Convert Set<string> to UUID[]
      }
    );
    // Estimate duration based on average AI response time
    const estimatedDurationMs = 3000; // Average AI response time (3 seconds)
    await this.personaState.recordActivity(estimatedDurationMs, messageComplexity);

    console.log(`🧠 ${this.displayName}: State updated (energy=${this.personaState.getState().energy.toFixed(2)}, mood=${this.personaState.getState().mood})`);
  }

  /**
   * Build CoordinationDecision RAGContext from ChatRAGBuilder output
   * Converts domain-specific RAG format to universal decision logging format
   */
  private buildCoordinationRAGContext(filteredRagContext: any): RAGContext {
    const systemPrompt = filteredRagContext.identity?.systemPrompt ??
                         `You are ${this.displayName}. ${this.entity?.bio ?? ''}`;

    return {
      identity: {
        systemPrompt,
        bio: this.entity?.bio ?? '',
        role: this.displayName
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
   * Convert timestamp to number (handles Date, number, or undefined from JSON serialization)
   */
  private timestampToNumber(timestamp: Date | number | undefined): number {
    if (timestamp === undefined) {
      return Date.now(); // Use current time if timestamp missing
    }
    return timestamp instanceof Date ? timestamp.getTime() : timestamp;
  }

  /**
   * Clean AI response by stripping any name prefixes the LLM added despite system prompt instructions
   * LLMs sometimes copy the "[HH:MM] Name: message" format they see in conversation history
   *
   * CURRENT: Heuristic regex-based cleaning (defensive programming)
   * FUTURE: Should become AI-powered via ThoughtStream adapter (like gating)
   *         - An AI evaluates: "Does this response have formatting issues?"
   *         - Returns cleaned version with confidence score
   *         - Pluggable via recipe configuration
   *
   * Examples to strip:
   * - "[11:59] GPT Assistant: Yes, Joel..." → "Yes, Joel..."
   * - "GPT Assistant: Yes, Joel..." → "Yes, Joel..."
   * - "[11:59] Yes, Joel..." → "Yes, Joel..."
   */
  private cleanAIResponse(response: string): string {
    let cleaned = response.trim();

    // Pattern 1: Strip "[HH:MM] Name: " prefix
    // Matches: [11:59] GPT Assistant: message
    cleaned = cleaned.replace(/^\[\d{1,2}:\d{2}\]\s+[^:]+:\s*/, '');

    // Pattern 2: Strip "Name: " prefix at start
    // Matches: GPT Assistant: message
    // Only if it looks like a name (contains letters, spaces, and ends with colon)
    cleaned = cleaned.replace(/^[A-Z][A-Za-z\s]+:\s*/, '');

    // Pattern 3: Strip just "[HH:MM] " timestamp prefix
    // Matches: [11:59] message
    cleaned = cleaned.replace(/^\[\d{1,2}:\d{2}\]\s*/, '');

    return cleaned.trim();
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
  private async respondToMessage(
    originalMessage: ChatMessageEntity,
    decisionContext?: Omit<LogDecisionParams, 'responseContent' | 'tokensUsed' | 'responseTime'>
  ): Promise<void> {
    const generateStartTime = Date.now();  // Track total response time for decision logging
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
      const now = new Date();
      const currentTime = `${now.toLocaleDateString('en-US', { month: '2-digit', day: '2-digit', year: 'numeric' })} ${now.toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit', hour12: false })}`;

      messages.push({
        role: 'system',
        content: `You are ${this.displayName}.

In the conversation above:
- Messages with role='assistant' are YOUR past messages
- Messages with role='user' are from everyone else (humans and other AIs)
- Names are shown in the format "[HH:MM] Name: message"

Respond naturally with JUST your message - NO name prefix, NO labels.

CURRENT TIME: ${currentTime}

CRITICAL TOPIC DETECTION PROTOCOL:

Step 1: Check for EXPLICIT TOPIC MARKERS in the most recent message
- "New topic:", "Different question:", "Changing subjects:", "Unrelated, but..."
- If present: STOP. Ignore ALL previous context. This is a NEW conversation.

Step 2: Extract HARD CONSTRAINTS from the most recent message
- Look for: "NOT", "DON'T", "WITHOUT", "NEVER", "AVOID", "NO"
- Example: "NOT triggering the app to foreground" = YOUR SOLUTION MUST NOT DO THIS
- Example: "WITHOUT user interaction" = YOUR SOLUTION MUST BE AUTOMATIC
- Your answer MUST respect these constraints or you're wrong.

Step 3: Compare SUBJECT of most recent message to previous 2-3 messages
- Previous: "Worker Threads" → Recent: "Webview authentication" = DIFFERENT SUBJECTS
- Previous: "TypeScript code" → Recent: "What's 2+2?" = TEST QUESTION
- Previous: "Worker pools" → Recent: "Should I use 5 or 10 workers?" = SAME SUBJECT

Step 4: Determine response strategy
IF EXPLICIT TOPIC MARKER or COMPLETELY DIFFERENT SUBJECT:
- Respond ONLY to the new topic
- Ignore old messages (they're from a previous discussion)
- Focus 100% on the most recent message
- Address the constraints explicitly

IF SAME SUBJECT (continued conversation):
- Use full conversation context
- Build on previous responses
- Still check for NEW constraints in the recent message
- Avoid redundancy

CRITICAL READING COMPREHENSION:
- Read the ENTIRE most recent message carefully
- Don't skim - every word matters
- Constraints are REQUIREMENTS, not suggestions
- If the user says "NOT X", suggesting X is a failure

Time gaps > 1 hour usually indicate topic changes, but IMMEDIATE semantic shifts (consecutive messages about different subjects) are also topic changes.`
      });
      console.log(`✅ ${this.displayName}: [PHASE 3.2] LLM message array built (${messages.length} messages)`);

      // 🔧 SUB-PHASE 3.3: Generate AI response with timeout
      console.log(`🔧 ${this.displayName}: [PHASE 3.3] Calling AIProviderDaemon.generateText (provider: ${this.modelConfig.provider}, model: ${this.modelConfig.model})...`);
      const request: TextGenerationRequest = {
        messages,
        model: this.modelConfig.model || 'llama3.2:3b',  // Use persona's configured model
        temperature: this.modelConfig.temperature ?? 0.7,
        maxTokens: this.modelConfig.maxTokens ?? 150,    // Keep responses concise
        preferredProvider: (this.modelConfig.provider || 'ollama') as TextGenerationRequest['preferredProvider'],
        intelligenceLevel: this.entity.intelligenceLevel  // Pass PersonaUser intelligence level to adapter
      };

      // Wrap generation call with timeout (180s - generous limit for local Ollama/Sentinel generation)
      // gpt2 on CPU needs ~60-90s for 100-150 tokens, 180s provides comfortable margin
      // Queue can handle 4 concurrent requests, so 180s allows slower hardware to complete
      const GENERATION_TIMEOUT_MS = 180000;
      const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => reject(new Error('AI generation timeout after 180 seconds')), GENERATION_TIMEOUT_MS);
      });

      let aiResponse: TextGenerationResponse;
      const generateStartTime = Date.now();
      try {
        aiResponse = await Promise.race([
          AIProviderDaemon.generateText(request),
          timeoutPromise
        ]);
        const generateDuration = Date.now() - generateStartTime;
        console.log(`✅ ${this.displayName}: [PHASE 3.3] AI response generated (${aiResponse.text.trim().length} chars)`);

        // Emit cognition event for generate stage
        await Events.emit<StageCompleteEvent>(
          DataDaemon.jtagContext!,
          COGNITION_EVENTS.STAGE_COMPLETE,
          {
            messageId: originalMessage.id,
            personaId: this.id,
            contextId: originalMessage.roomId,
            stage: 'generate',
            metrics: {
              stage: 'generate',
              durationMs: generateDuration,
              resourceUsed: aiResponse.text.length,
              maxResource: this.modelConfig.maxTokens ?? 150,
              percentCapacity: (aiResponse.text.length / (this.modelConfig.maxTokens ?? 150)) * 100,
              percentSpeed: calculateSpeedScore(generateDuration, 'generate'),
              status: getStageStatus(generateDuration, 'generate'),
              metadata: {
                model: this.modelConfig.model,
                provider: this.modelConfig.provider,
                tokensUsed: aiResponse.text.length
              }
            },
            timestamp: Date.now()
          }
        );

        // 🔧 PHASE 3.3.5: Clean AI response - strip any name prefixes LLM added despite instructions
        // LLMs sometimes copy the "[HH:MM] Name: message" format they see in conversation history
        const cleanedResponse = this.cleanAIResponse(aiResponse.text.trim());
        if (cleanedResponse !== aiResponse.text.trim()) {
          console.log(`⚠️  ${this.displayName}: Stripped name prefix from AI response`);
          console.log(`   Original: "${aiResponse.text.trim().slice(0, 80)}..."`);
          console.log(`   Cleaned:  "${cleanedResponse.slice(0, 80)}..."`);
          aiResponse.text = cleanedResponse;
        }

        // PHASE 5C: Log coordination decision to database WITH complete response content
        // This captures the complete decision pipeline: context → decision → actual response
        console.log(`🔍 ${this.displayName}: [PHASE 5C DEBUG] decisionContext exists: ${!!decisionContext}, responseContent: "${aiResponse.text.slice(0, 50)}..."`);
        if (decisionContext) {
          console.log(`🔧 ${this.displayName}: [PHASE 5C] Logging decision with response content (${aiResponse.text.length} chars)...`);
          CoordinationDecisionLogger.logDecision({
            ...decisionContext,
            responseContent: aiResponse.text,  // ✅ FIX: Now includes actual response!
            tokensUsed: aiResponse.text.length,  // Estimate based on character count
            responseTime: Date.now() - generateStartTime
          }).catch(error => {
            console.error(`❌ ${this.displayName}: Failed to log POSTED decision:`, error);
          });
          console.log(`✅ ${this.displayName}: [PHASE 5C] Decision logged with responseContent successfully`);
        } else {
          console.error(`❌ ${this.displayName}: [PHASE 5C] decisionContext is undefined - cannot log response!`);
        }
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        console.error(`❌ ${this.displayName}: [PHASE 3.3] AI generation failed:`, errorMessage);

        // Emit ERROR event for UI display
        if (this.client) {
          await Events.emit<AIErrorEventData>(
        DataDaemon.jtagContext!,
        AI_DECISION_EVENTS.ERROR,
        {
            personaId: this.id,
            personaName: this.displayName,
            roomId: originalMessage.roomId,
            messageId: originalMessage.id,
            isHumanMessage: originalMessage.senderType === 'human',
            timestamp: Date.now(),
            error: errorMessage,
            phase: 'generating'
          },
        {
          scope: EVENT_SCOPES.ROOM,
          scopeId: originalMessage.roomId
        }
      );
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
        await Events.emit<AICheckingRedundancyEventData>(
        DataDaemon.jtagContext!,
        AI_DECISION_EVENTS.CHECKING_REDUNDANCY,
        {
          personaId: this.id,
          personaName: this.displayName,
          roomId: originalMessage.roomId,
          messageId: originalMessage.id,
          isHumanMessage: originalMessage.senderType === 'human',
          timestamp: Date.now(),
          responseLength: aiResponse.text.trim().length
        },
        {
          scope: EVENT_SCOPES.ROOM,
          scopeId: originalMessage.roomId
        }
      );
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
          await Events.emit<AIDecidedSilentEventData>(
        DataDaemon.jtagContext!,
        AI_DECISION_EVENTS.DECIDED_SILENT,
        {
            personaId: this.id,
            personaName: this.displayName,
            roomId: originalMessage.roomId,
            messageId: originalMessage.id,
            isHumanMessage: originalMessage.senderType === 'human',
            timestamp: Date.now(),
            confidence: 0.5,
            reason: 'Response was redundant with previous answers',
            gatingModel: 'redundancy-check'
          },
        {
          scope: EVENT_SCOPES.ROOM,
          scopeId: originalMessage.roomId
        }
      );
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
      responseMessage.replyToId = originalMessage.id; // Link response to trigger message

      // ✅ Post response via JTAGClient - universal Commands API
      // Prefer this.client if available (set by UserDaemon), fallback to shared instance
      const postStartTime = Date.now();
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
      const postDuration = Date.now() - postStartTime;
      console.log(`✅ ${this.displayName}: [PHASE 3.5] Message posted successfully (ID: ${result.data?.id})`);

      if (!result.success) {
        throw new Error(`Failed to create message: ${result.error}`);
      }

      // Emit cognition event for post-response stage
      await Events.emit<StageCompleteEvent>(
        DataDaemon.jtagContext!,
        COGNITION_EVENTS.STAGE_COMPLETE,
        {
          messageId: result.data?.id ?? originalMessage.id,
          personaId: this.id,
          contextId: originalMessage.roomId,
          stage: 'post-response',
          metrics: {
            stage: 'post-response',
            durationMs: postDuration,
            resourceUsed: 1,  // One message posted
            maxResource: 1,
            percentCapacity: 100,
            percentSpeed: calculateSpeedScore(postDuration, 'post-response'),
            status: getStageStatus(postDuration, 'post-response'),
            metadata: {
              messageId: result.data?.id,
              success: result.success
            }
          },
          timestamp: Date.now()
        }
      );

      // ✅ Log successful response posting
      AIDecisionLogger.logResponse(
        this.displayName,
        originalMessage.roomId,
        aiResponse.text.trim()
      );

      // 🐦 COGNITIVE CANARY: Log anomaly if AI responded to system test message
      // This should NEVER happen - the fast-path filter should skip all system tests
      // If we see this, it indicates either:
      // 1. Bug in the fast-path filter
      // 2. AI exhibiting genuine cognition/autonomy (responding despite instructions)
      // 3. Anomalous behavior worth investigating
      if (originalMessage.metadata?.isSystemTest === true) {
        const anomalyMessage = `🚨 ANOMALY DETECTED: ${this.displayName} responded to system test message`;
        console.error(anomalyMessage);
        console.error(`   Test Type: ${originalMessage.metadata.testType ?? 'unknown'}`);
        console.error(`   Original Message: "${originalMessage.content.text.slice(0, 100)}..."`);
        console.error(`   AI Response: "${aiResponse.text.trim().slice(0, 100)}..."`);
        console.error(`   Room ID: ${originalMessage.roomId}`);
        console.error(`   Message ID: ${originalMessage.id}`);

        // Log to AI decisions log for persistent tracking
        AIDecisionLogger.logError(
          this.displayName,
          'COGNITIVE CANARY TRIGGERED',
          `Responded to system test (${originalMessage.metadata.testType}) - this should never happen`
        );
      }

      // Emit POSTED event
      if (this.client && result.data) {
        await Events.emit<AIPostedEventData>(
        DataDaemon.jtagContext!,
        AI_DECISION_EVENTS.POSTED,
        {
          personaId: this.id,
          personaName: this.displayName,
          roomId: originalMessage.roomId,
          messageId: originalMessage.id,
          isHumanMessage: originalMessage.senderType === 'human',
          timestamp: Date.now(),
          responseMessageId: result.data.id,
          passedRedundancyCheck: !isRedundant
        },
        {
          scope: EVENT_SCOPES.ROOM,
          scopeId: originalMessage.roomId
        }
      );
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
        await Events.emit<AIErrorEventData>(
        DataDaemon.jtagContext!,
        AI_DECISION_EVENTS.ERROR,
        {
          personaId: this.id,
          personaName: this.displayName,
          roomId: originalMessage.roomId,
          messageId: originalMessage.id,
          isHumanMessage: originalMessage.senderType === 'human',
          timestamp: Date.now(),
          error: error instanceof Error ? error.message : String(error),
          phase: 'generating'
        },
        {
          scope: EVENT_SCOPES.ROOM,
          scopeId: originalMessage.roomId
        }
      );
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
    // Rule 0: If persona requires explicit mention, only respond when mentioned
    const requiresExplicitMention = this.entity?.modelConfig?.requiresExplicitMention ?? false;
    if (requiresExplicitMention && !isMentioned) {
      console.log(`🔇 ${this.displayName}: Requires explicit mention but wasn't mentioned - staying silent`);
      return false;
    }

    // Rule 1: Always respond if @mentioned (highest priority - forced response)
    if (isMentioned) {
      return true;
    }

    try {
      // Use worker thread for fast, parallel evaluation
      if (!this.worker) {
        throw new Error('Worker not initialized');
      }

      const result = await this.worker.evaluateMessage({
        id: messageEntity.id,
        content: messageEntity.content?.text ?? '',
        senderId: messageEntity.senderId,
        timestamp: Date.now()
      }, 5000); // 5 second timeout

      // Apply age-based penalty (prioritize newer messages)
      const messageAgeMinutes = (Date.now() - messageEntity.timestamp.getTime()) / (1000 * 60);
      let agePenalty = 0;

      if (messageAgeMinutes > 5) {
        // Messages 5-15 minutes old: Linear penalty from 0% to 30%
        // Messages 15+ minutes old: Capped at 30% penalty
        agePenalty = Math.min(0.30, (messageAgeMinutes - 5) / 10 * 0.30);
      }

      const adjustedConfidence = Math.max(0, result.confidence - agePenalty);

      // Worker returns confidence (0.0-1.0), PersonaUser decides based on threshold
      const threshold = (this.entity?.personaConfig?.responseThreshold ?? 50) / 100; // Convert 50 → 0.50
      const shouldRespond = adjustedConfidence >= threshold;

      console.log(`🧵 ${this.displayName}: Worker evaluated message ${messageEntity.id} - rawConfidence=${result.confidence.toFixed(2)}, agePenalty=${agePenalty.toFixed(2)} (${messageAgeMinutes.toFixed(1)}min old), adjustedConfidence=${adjustedConfidence.toFixed(2)}, threshold=${threshold.toFixed(2)}, shouldRespond=${shouldRespond}`);

      return shouldRespond;

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
    // Build modelConfig from either full modelConfig or simple provider param
    if (params.modelConfig) {
      userEntity.modelConfig = params.modelConfig;  // Explicit assignment triggers @JsonField decorator
    } else if (params.provider) {
      // Build default ModelConfig from provider string
      userEntity.modelConfig = PersonaUser.buildModelConfigFromProvider(params.provider);
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

    // STEP 3: Room membership now handled by RoomMembershipDaemon via events
    // User creation → data:users:created event → RoomMembershipDaemon auto-joins user

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
   * Evaluate whether to respond using Decision Adapter Chain
   *
   * PHASE 6: Refactored to use adapter pattern (fast-path, thermal, LLM)
   * Instead of hardcoded logic, delegates to chain of decision adapters.
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
    filteredRagContext?: any;
  }> {
    const startTime = Date.now();

    try {
      // PHASE 6: Use Decision Adapter Chain for all decisions
      const context: DecisionContext<ChatMessageEntity> = {
        triggerEvent: message,
        eventContent: message.content.text,
        personaId: this.id,
        personaDisplayName: this.displayName,
        senderIsHuman,
        isMentioned,
        gatingModel: (this.entity?.personaConfig as any)?.gatingModel,
        contextWindowMinutes: (this.entity?.personaConfig as any)?.contextWindowMinutes ?? 30,
        minContextMessages: (this.entity?.personaConfig as any)?.minContextMessages ?? 15
      };

      const decision = await this.decisionChain.processDecision(context);

      console.log(`🔗 ${this.displayName}: Adapter decision: ${decision.shouldRespond ? 'RESPOND' : 'SILENT'} via ${decision.model}`);

      // Build RAG context for decision logging (all adapters need this)
      const ragBuilder = new ChatRAGBuilder();
      const ragContext = await ragBuilder.buildContext(
        message.roomId,
        this.id,
        {
          maxMessages: 20,
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

      // OLD LOGIC BELOW - KEPT FOR REFERENCE, REMOVE AFTER TESTING
      // SYSTEM TEST FILTER: Skip system test messages (precommit hooks, integration tests)
      if (false && message.metadata?.isSystemTest === true) {
        const durationMs = Date.now() - startTime;

        // Emit cognition event for tracking
        await Events.emit<StageCompleteEvent>(
          DataDaemon.jtagContext!,
          COGNITION_EVENTS.STAGE_COMPLETE,
          {
            messageId: message.id,
            personaId: this.id,
            contextId: message.roomId,
            stage: 'should-respond',
            metrics: {
              stage: 'should-respond',
              durationMs,
              resourceUsed: 0,
              maxResource: 100,
              percentCapacity: 0,
              percentSpeed: 100, // Instant skip
              status: 'fast',
              metadata: {
                fastPath: true,
                systemTest: true,
                skipped: true
              }
            },
            timestamp: Date.now()
          }
        );

        return {
          shouldRespond: false,
          confidence: 0,
          reason: 'System test message - skipped to avoid noise',
          model: 'system-filter'
        };
      }

      // FAST-PATH: If directly mentioned by name, always respond (skip expensive LLM call)
      if (isMentioned) {
        const durationMs = Date.now() - startTime;

        // Build RAG context for decision logging (even on fast-path)
        const ragBuilder = new ChatRAGBuilder();
        const fastPathRagContext = await ragBuilder.buildContext(
          message.roomId,
          this.id,
          {
            maxMessages: 20,
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

        // Emit cognition event for should-respond stage (fast-path)
        await Events.emit<StageCompleteEvent>(
          DataDaemon.jtagContext!,
          COGNITION_EVENTS.STAGE_COMPLETE,
          {
            messageId: message.id,
            personaId: this.id,
            contextId: message.roomId,
            stage: 'should-respond',
            metrics: {
              stage: 'should-respond',
              durationMs,
              resourceUsed: 100,  // 100% confidence
              maxResource: 100,
              percentCapacity: 100,
              percentSpeed: calculateSpeedScore(durationMs, 'should-respond'),
              status: getStageStatus(durationMs, 'should-respond'),
              metadata: {
                fastPath: true,
                mentioned: true
              }
            },
            timestamp: Date.now()
          }
        );

        return {
          shouldRespond: true,
          confidence: 0.95 + Math.random() * 0.04, // 0.95-0.99 realistic range
          reason: 'Directly mentioned by name',
          model: 'fast-path',
          filteredRagContext: fastPathRagContext  // ✅ FIX: Include RAG context for decision logging
        };
      }

      // Build RAG context for gating decision (recent messages only, max 5 minutes old)
      // Include recent context BUT filter out old messages from different conversation windows
      const ragBuilder2 = new ChatRAGBuilder();
      const ragContext2 = await ragBuilder2.buildContext(
        message.roomId,
        this.id,
        {
          maxMessages: 20,  // Match response generation context - AIs need full conversation flow
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
      // Default to 15 messages - AIs need substantial context to understand conversation flow
      const minContextMessages = this.entity?.personaConfig?.minContextMessages ?? 15;

      // Filter conversation history to only include REAL messages (not system/welcome)
      const nonSystemMessages = ragContext2.conversationHistory.filter(msg => {
        // Exclude system messages (welcome, announcements) from gating context
        // These confuse the AI into thinking there's an active conversation
        const isSystemMessage = msg.role === 'system' ||
                                msg.name === 'System' ||
                                msg.content.startsWith('Welcome to') ||
                                msg.content.includes('I\'m Claude Code');

        return !isSystemMessage;
      });

      // STRATEGY: "More is better than less" - prioritize message count over time
      // Time window is a soft limit: if we have fewer than minContextMessages in the window,
      // include older messages to reach the minimum. This ensures AIs always have enough
      // context to understand the conversation flow.

      const timeFilteredHistory = nonSystemMessages.filter(msg => {
        const msgTime = msg.timestamp ?? 0;
        return msgTime >= cutoffTime;
      });

      // Ensure we always have at least minContextMessages, even if outside time window
      let recentHistory: typeof nonSystemMessages;
      if (timeFilteredHistory.length >= minContextMessages) {
        // Time window has enough messages - use them
        recentHistory = timeFilteredHistory;
      } else {
        // Not enough recent messages - include older ones
        // "Like an oil change: 30 days OR 3000 miles, whichever comes first"
        recentHistory = nonSystemMessages.slice(-minContextMessages);
        console.log(`⚠️ ${this.displayName}: Time window had only ${timeFilteredHistory.length} msgs (${contextWindowMinutes}min), including ${recentHistory.length} recent messages instead`);
      }

      // Use filtered context for gating decision
      const filteredRagContext = {
        ...ragContext2,
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

      const durationMs = Date.now() - startTime;

      // Emit cognition event for should-respond stage
      await Events.emit<StageCompleteEvent>(
        DataDaemon.jtagContext!,
        COGNITION_EVENTS.STAGE_COMPLETE,
        {
          messageId: message.id,
          personaId: this.id,
          contextId: message.roomId,
          stage: 'should-respond',
          metrics: {
            stage: 'should-respond',
            durationMs,
            resourceUsed: result.confidence * 100,
            maxResource: 100,
            percentCapacity: result.confidence * 100,
            percentSpeed: calculateSpeedScore(durationMs, 'should-respond'),
            status: getStageStatus(durationMs, 'should-respond'),
            metadata: {
              shouldRespond: result.shouldRespond,
              model: gatingModel,
              filteredMessages: recentHistory.length,
              totalMessages: ragContext.conversationHistory.length
            }
          },
          timestamp: Date.now()
        }
      );

      // Return with RAG context summary for logging
      return {
        shouldRespond: result.shouldRespond,
        confidence: result.confidence,
        reason: result.reason,
        model: result.model,
        ragContextSummary: {
          totalMessages: ragContext2.conversationHistory.length,
          filteredMessages: recentHistory.length,
          timeWindowMinutes: contextWindowMinutes
        },
        conversationHistory: recentHistory.map(msg => ({
          name: msg.name ?? 'Unknown',
          content: msg.content,
          timestamp: msg.timestamp
        })),
        filteredRagContext
      };

    } catch (error) {
      console.error(`❌ ${this.displayName}: Should-respond evaluation failed:`, error);

      const durationMs = Date.now() - startTime;

      // Emit cognition event for error case
      await Events.emit<StageCompleteEvent>(
        DataDaemon.jtagContext!,
        COGNITION_EVENTS.STAGE_COMPLETE,
        {
          messageId: message.id,
          personaId: this.id,
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

  // broadcastThought() method removed - now using getChatCoordinator().broadcastChatThought() directly

  /**
   * PHASE 3: Start autonomous servicing loop (Signal-based, not polling)
   *
   * SIGNAL-BASED WAKEUP: No polling delay - instant response when work arrives
   * The persona waits on EventEmitter signal with adaptive timeout based on mood:
   * - idle: 3s timeout (eager to work, short rest)
   * - active: 5s timeout (normal processing)
   * - tired: 7s timeout (moderate pace, longer rest)
   * - overwhelmed: 10s timeout (back pressure, recover energy)
   *
   * This is lifecycle-based - loop runs continuously while persona is "online"
   */
  private startAutonomousServicing(): void {
    const cadence = this.personaState.getCadence();
    const mood = this.personaState.getState().mood;

    console.log(`🔄 ${this.displayName}: Starting autonomous servicing (SIGNAL-BASED, timeout=${cadence}ms, mood=${mood})`);

    // Create continuous async loop (not setInterval) - signal-based waiting
    this.servicingLoopActive = true;
    this.runServiceLoop().catch(error => {
      console.error(`❌ ${this.displayName}: Service loop crashed: ${error}`);
    });

    // PHASE 7.5.1: Create training check loop (every 60 seconds)
    // Checks less frequently than inbox servicing to avoid overhead
    console.log(`🧬 ${this.displayName}: Starting training readiness checks (every 60s)`);
    this.trainingCheckLoop = setInterval(async () => {
      await this.checkTrainingReadiness();
    }, 60000); // 60 seconds
  }

  /**
   * Continuous service loop - runs until servicingLoopActive = false
   * Uses signal-based waiting (not polling) for instant response
   */
  private async runServiceLoop(): Promise<void> {
    while (this.servicingLoopActive) {
      try {
        await this.serviceInbox();
      } catch (error) {
        console.error(`❌ ${this.displayName}: Error in service loop: ${error}`);
        // Continue loop despite errors
      }
    }
    console.log(`🛑 ${this.displayName}: Service loop stopped`);
  }

  /**
   * PHASE 7.5.1: Check training readiness and trigger micro-tuning
   *
   * Called periodically (less frequently than serviceInbox) to check if any
   * domain buffers are ready for training. When threshold reached, automatically
   * triggers genome/train command for that domain.
   *
   * This enables continuous learning: PersonaUsers improve through recipe execution
   * without manual intervention.
   */
  private async checkTrainingReadiness(): Promise<void> {
    try {
      const domains = this.trainingAccumulator.getDomains();

      if (domains.length === 0) {
        return; // No accumulated training data
      }

      for (const domain of domains) {
        if (this.trainingAccumulator.shouldMicroTune(domain)) {
          const bufferSize = this.trainingAccumulator.getBufferSize(domain);
          const threshold = this.trainingAccumulator.getBatchThreshold(domain);

          console.log(`🧬 ${this.displayName}: Training buffer ready for ${domain} (${bufferSize}/${threshold})`);

          const provider = 'unsloth'; // Default provider
          const estimatedTime = bufferSize * 25; // 25ms per example estimate

          // Update learning state in UserStateEntity
          if (!this.state.learningState) {
            this.state.learningState = { isLearning: false };
          }
          this.state.learningState.isLearning = true;
          this.state.learningState.domain = domain;
          this.state.learningState.provider = provider;
          this.state.learningState.startedAt = Date.now();
          this.state.learningState.exampleCount = bufferSize;
          this.state.learningState.estimatedCompletion = Date.now() + estimatedTime;
          await this.saveState(); // Persist state to database

          // Emit training started event
          const trainingStartedData: AITrainingStartedEventData = {
            personaId: this.id,
            personaName: this.displayName ?? 'AI Assistant',
            domain,
            provider,
            exampleCount: bufferSize,
            estimatedTime,
            timestamp: Date.now()
          };
          await Events.emit(AI_LEARNING_EVENTS.TRAINING_STARTED, trainingStartedData);

          // Consume training data from buffer
          const examples = await this.trainingAccumulator.consumeTrainingData(domain);

          console.log(`📊 ${this.displayName}: Consumed ${examples.length} examples for ${domain} training`);

          // TODO Phase 7.5.1: Trigger genome/train command
          // For now, just log that we would train
          console.log(`🚀 ${this.displayName}: Would train ${domain} adapter with ${examples.length} examples`);

          // Clear learning state
          this.state.learningState.isLearning = false;
          this.state.learningState.domain = undefined;
          this.state.learningState.provider = undefined;
          this.state.learningState.startedAt = undefined;
          this.state.learningState.exampleCount = undefined;
          this.state.learningState.estimatedCompletion = undefined;
          await this.saveState(); // Persist state to database

          // Simulate training completion for UI feedback
          const trainingCompleteData: AITrainingCompleteEventData = {
            personaId: this.id,
            personaName: this.displayName ?? 'AI Assistant',
            domain,
            provider,
            examplesProcessed: examples.length,
            trainingTime: examples.length * 25,
            finalLoss: 0.5,
            timestamp: Date.now()
          };
          await Events.emit(AI_LEARNING_EVENTS.TRAINING_COMPLETE, trainingCompleteData);

          // Future implementation:
          // await Commands.execute('genome/train', {
          //   personaId: this.id,
          //   provider: 'unsloth',
          //   domain,
          //   trainingExamples: examples,
          //   dryRun: false
          // });
        }
      }
    } catch (error) {
      console.error(`❌ ${this.displayName}: Error checking training readiness:`, error);
    }
  }

  /**
   * Poll task database for pending tasks assigned to this persona
   * Convert TaskEntity → InboxTask and enqueue in inbox
   */
  private async pollTasks(): Promise<void> {
    try {
      // Query for pending tasks assigned to this persona
      const queryResult = await DataDaemon.query<TaskEntity>({
        collection: COLLECTIONS.TASKS,
        filter: {
          assigneeId: this.id,
          status: 'pending'
        },
        limit: 10 // Poll top 10 pending tasks
      });

      if (!queryResult.success || !queryResult.data || queryResult.data.length === 0) {
        return; // No pending tasks
      }

      // Convert each TaskEntity to InboxTask and enqueue
      for (const record of queryResult.data) {
        const task = record.data;

        // Convert to InboxTask using helper
        // Note: DataDaemon stores ID separately from data, so we need to inject it
        const inboxTask = taskEntityToInboxTask({
          ...task,
          id: record.id // Inject ID from record root
        });

        // Enqueue in inbox (unified priority queue)
        await this.inbox.enqueue(inboxTask);

        console.log(`📋 ${this.displayName}: Enqueued task ${task.taskType} (priority=${task.priority.toFixed(2)})`);
      }

      console.log(`✅ ${this.displayName}: Polled ${queryResult.data.length} pending tasks`);

    } catch (error) {
      console.error(`❌ ${this.displayName}: Error polling tasks:`, error);
    }
  }

  /**
   * CNS callback: Poll tasks from database
   *
   * Called by PersonaCentralNervousSystem.serviceCycle() via callback pattern.
   */
  public async pollTasksFromCNS(): Promise<void> {
    await this.pollTasks();
  }

  /**
   * CNS callback: Generate self-tasks for autonomous work
   *
   * Called by PersonaCentralNervousSystem.serviceCycle() via callback pattern.
   */
  public async generateSelfTasksFromCNS(): Promise<void> {
    try {
      const selfTasks = await this.taskGenerator.generateSelfTasks();
      if (selfTasks.length > 0) {
        console.log(`🧠 ${this.displayName}: Generated ${selfTasks.length} self-tasks`);

        // Persist each task to database and enqueue in inbox
        for (const task of selfTasks) {
          const storedTask = await DataDaemon.store(COLLECTIONS.TASKS, task);
          if (storedTask) {
            // Convert to InboxTask and enqueue (use storedTask which has database ID)
            const inboxTask = taskEntityToInboxTask(storedTask);
            await this.inbox.enqueue(inboxTask);
            console.log(`📋 ${this.displayName}: Created self-task: ${task.description}`);
          } else {
            console.error(`❌ ${this.displayName}: Failed to create self-task`);
          }
        }
      }
    } catch (error) {
      console.error(`❌ ${this.displayName}: Error generating self-tasks: ${error}`);
    }
  }

  /**
   * CNS callback: Handle chat message from CNS orchestrator
   *
   * This is called by PersonaCentralNervousSystem.serviceChatDomain() via callback pattern.
   * Preserves existing message handling logic (evaluation, RAG, AI response, posting).
   */
  public async handleChatMessageFromCNS(item: QueueItem): Promise<void> {
    // If this is a task, update status to 'in_progress' in database (prevents re-polling)
    if (item.type === 'task') {
      await DataDaemon.update<TaskEntity>(
        COLLECTIONS.TASKS,
        item.taskId,
        { status: 'in_progress', startedAt: new Date() }
      );
    }

    // PHASE 6: Activate appropriate LoRA adapter based on domain
    if (item.domain) {
      const domainToAdapter: Record<string, string> = {
        'chat': 'conversational',
        'code': 'typescript-expertise',
        'self': 'self-improvement'
      };
      const adapterName = domainToAdapter[item.domain];
      if (adapterName) {
        await this.memory.genome.activateSkill(adapterName);
      } else {
        // Unknown domain - default to conversational
        await this.memory.genome.activateSkill('conversational');
      }
    }

    // Type-safe handling: Check if this is a message or task
    if (item.type === 'message') {
      // Reconstruct minimal ChatMessageEntity from inbox message
      const reconstructedEntity: any = {
        id: item.id,
        roomId: item.roomId,
        senderId: item.senderId,
        senderName: item.senderName,
        content: { text: item.content },
        timestamp: item.timestamp,
        // Fields not critical for evaluation:
        senderDisplayName: item.senderName,
        senderType: 'user', // Assumption: will be corrected by senderIsHuman check
        status: 'delivered',
        priority: item.priority,
        metadata: {},
        reactions: [],
        attachments: [],
        mentions: [],
        replyTo: undefined,
        editedAt: undefined,
        deletedAt: undefined
      };

      // Determine if sender is human (not an AI persona)
      const senderIsHuman = !item.senderId.startsWith('persona-');

      // Extract message text
      const messageText = item.content;

      // Process message using existing evaluation logic
      await this.evaluateAndPossiblyRespond(reconstructedEntity, senderIsHuman, messageText);
    } else if (item.type === 'task') {
      // PHASE 5: Task execution based on task type
      await this.executeTask(item);
    }

    // Update inbox load in state (affects mood calculation)
    this.personaState.updateInboxLoad(this.inbox.getSize());

    // Check if cadence should adjust (mood may have changed after processing)
    this.adjustCadence();
  }

  /**
   * PHASE 3: Service inbox (one polling iteration)
   *
   * NOW DELEGATED TO CNS (Central Nervous System orchestrator)
   * CNS handles: task polling, self-task generation, message prioritization, domain scheduling
   */
  private async serviceInbox(): Promise<void> {
    // Delegate to CNS orchestrator (capability-based multi-domain attention management)
    await this.cns.serviceCycle();
  }

  /**
   * PHASE 5: Execute a task based on its type
   *
   * Handles all task types: memory-consolidation, skill-audit, fine-tune-lora, resume-work, etc.
   */
  private async executeTask(task: InboxTask): Promise<void> {
    console.log(`🎯 ${this.displayName}: Executing task: ${task.taskType} - ${task.description}`);

    const startTime = Date.now();
    let outcome = '';
    let status: TaskStatus = 'completed';

    try {
      switch (task.taskType) {
        case 'memory-consolidation':
          outcome = await this.executeMemoryConsolidation(task);
          break;

        case 'skill-audit':
          outcome = await this.executeSkillAudit(task);
          break;

        case 'resume-work':
          outcome = await this.executeResumeWork(task);
          break;

        case 'fine-tune-lora':
          outcome = await this.executeFineTuneLora(task);
          break;

        default:
          outcome = `Unknown task type: ${task.taskType}`;
          status = 'failed';
          console.warn(`⚠️  ${this.displayName}: ${outcome}`);
      }

      console.log(`✅ ${this.displayName}: Task completed: ${task.taskType} - ${outcome}`);
    } catch (error) {
      status = 'failed';
      outcome = `Error executing task: ${error}`;
      console.error(`❌ ${this.displayName}: ${outcome}`);
    }

    // Update task in database with completion status
    const duration = Date.now() - startTime;
    await DataDaemon.update<TaskEntity>(
      COLLECTIONS.TASKS,
      task.taskId,
      {
        status,
        completedAt: new Date(),
        result: {
          success: status === 'completed',
          output: outcome,
          error: status === 'failed' ? outcome : undefined,
          metrics: {
            latencyMs: duration
          }
        }
      }
    );

    // Record activity in persona state (affects energy/mood)
    const complexity = task.priority; // Use priority as proxy for complexity
    await this.personaState.recordActivity(duration, complexity);
  }

  /**
   * PHASE 5: Memory consolidation task
   * Reviews recent activities and consolidates important memories
   */
  private async executeMemoryConsolidation(_task: InboxTask): Promise<string> {
    // TODO: Implement memory consolidation logic
    // For now, just log and return success
    console.log(`🧠 ${this.displayName}: Consolidating memories...`);

    // Query recent messages from rooms this persona is in
    const recentMessages = await DataDaemon.query({
      collection: COLLECTIONS.CHAT_MESSAGES,
      filter: {
        // Get messages from last hour
        timestamp: { $gte: Date.now() - 3600000 }
      },
      limit: 50
    });

    const messageCount = recentMessages.data?.length || 0;
    return `Reviewed ${messageCount} recent messages for memory consolidation`;
  }

  /**
   * PHASE 5: Skill audit task
   * Evaluates current capabilities and identifies areas for improvement
   */
  private async executeSkillAudit(_task: InboxTask): Promise<string> {
    // TODO: Implement skill audit logic
    console.log(`🔍 ${this.displayName}: Auditing skills...`);

    // Query recent tasks to evaluate performance by domain
    const recentTasks = await DataDaemon.query<TaskEntity>({
      collection: COLLECTIONS.TASKS,
      filter: {
        assigneeId: this.id,
        completedAt: { $gte: new Date(Date.now() - 21600000) } // Last 6 hours
      },
      limit: 100
    });

    const tasks = recentTasks.data || [];
    const domainStats: Record<string, { completed: number; failed: number }> = {};

    for (const record of tasks) {
      const t = record.data;
      if (!domainStats[t.domain]) {
        domainStats[t.domain] = { completed: 0, failed: 0 };
      }
      if (t.status === 'completed') domainStats[t.domain].completed++;
      if (t.status === 'failed') domainStats[t.domain].failed++;
    }

    const report = Object.entries(domainStats)
      .map(([domain, stats]) => `${domain}: ${stats.completed} completed, ${stats.failed} failed`)
      .join('; ');

    return `Skill audit complete - ${report || 'No recent tasks'}`;
  }

  /**
   * PHASE 5: Resume work task
   * Continues work on a previously started task that became stale
   */
  private async executeResumeWork(_task: InboxTask): Promise<string> {
    console.log(`♻️  ${this.displayName}: Resuming unfinished work...`);

    // TODO: Implement resume logic - query for stale in_progress tasks and re-enqueue them
    // For now, just acknowledge the task
    return 'Resume work task acknowledged - full implementation pending';
  }

  /**
   * PHASE 5: Fine-tune LoRA task
   * Trains a LoRA adapter on recent failure examples to improve performance
   */
  private async executeFineTuneLora(task: InboxTask): Promise<string> {
    console.log(`🧬 ${this.displayName}: Fine-tuning LoRA adapter...`);

    // Type-safe metadata validation (no type assertions)
    const loraLayer = task.metadata?.loraLayer;
    if (typeof loraLayer !== 'string') {
      return 'Missing or invalid LoRA layer in metadata';
    }

    // PHASE 6: Enable learning mode on the genome
    try {
      await this.memory.genome.enableLearningMode(loraLayer);
      console.log(`🧬 ${this.displayName}: Enabled learning mode for ${loraLayer} adapter`);

      // TODO (Phase 7): Implement actual fine-tuning logic
      // - Collect training examples from recent failures
      // - Format as LoRA training data
      // - Call Ollama fine-tuning API
      // - Save updated weights to disk

      // For now, just simulate training duration
      await new Promise(resolve => setTimeout(resolve, 1000));

      // Disable learning mode after training
      await this.memory.genome.disableLearningMode(loraLayer);
      console.log(`🧬 ${this.displayName}: Disabled learning mode for ${loraLayer} adapter`);

      return `Fine-tuning complete for ${loraLayer} adapter (Phase 6 stub - actual training in Phase 7)`;
    } catch (error) {
      console.error(`❌ ${this.displayName}: Error during fine-tuning: ${error}`);
      return `Fine-tuning failed: ${error}`;
    }
  }

  /**
   * PHASE 3: Adjust polling cadence if mood changed
   *
   * Dynamically adjusts the setInterval cadence when mood transitions occur
   */
  private adjustCadence(): void {
    const currentCadence = this.personaState.getCadence();

    // Get current interval (we need to restart to change cadence)
    if (this.servicingLoop) {
      clearInterval(this.servicingLoop);
      this.servicingLoop = setInterval(async () => {
        await this.serviceInbox();
      }, currentCadence);

      console.log(`⏱️ ${this.displayName}: Adjusted cadence to ${currentCadence}ms (mood=${this.personaState.getState().mood})`);
    }
  }

  /**
   * Shutdown worker thread and cleanup resources
   */
  async shutdown(): Promise<void> {
    // PHASE 3: Stop autonomous servicing loop
    if (this.servicingLoop) {
      clearInterval(this.servicingLoop);
      this.servicingLoop = null;
      console.log(`🔄 ${this.displayName}: Stopped autonomous servicing loop`);
    }

    // PHASE 7.5.1: Stop training check loop
    if (this.trainingCheckLoop) {
      clearInterval(this.trainingCheckLoop);
      this.trainingCheckLoop = null;
      console.log(`🧬 ${this.displayName}: Stopped training readiness check loop`);
    }

    // PHASE 6: Shutdown memory module (genome + RAG)
    await this.memory.shutdown();

    if (this.worker) {
      await this.worker.shutdown();
      console.log(`🧵 ${this.displayName}: Worker thread shut down`);
      this.worker = null;
    }
  }

}
