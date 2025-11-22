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
import { generateUUID } from '../../core/types/CrossPlatformUUID';
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
import type { TextGenerationRequest, TextGenerationResponse, ChatMessage } from '../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
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
import { WorkingMemoryManager } from './modules/cognition/memory/WorkingMemoryManager';
import { PersonaSelfState } from './modules/cognition/PersonaSelfState';
import { SimplePlanFormulator } from './modules/cognition/reasoning/SimplePlanFormulator';
import type { Task, Plan } from './modules/cognition/reasoning/types';
import { CognitionLogger } from './modules/cognition/CognitionLogger';
import { PersonaToolExecutor } from './modules/PersonaToolExecutor';
import { PersonaTaskExecutor } from './modules/PersonaTaskExecutor';
import { PersonaTrainingManager } from './modules/PersonaTrainingManager';
import { PersonaAutonomousLoop } from './modules/PersonaAutonomousLoop';
import { PersonaResponseGenerator } from './modules/PersonaResponseGenerator';
import { PersonaMessageEvaluator } from './modules/PersonaMessageEvaluator';
import { PersonaGenomeManager } from './modules/PersonaGenomeManager';
import { type PersonaMediaConfig, DEFAULT_MEDIA_CONFIG } from './modules/PersonaMediaConfig';
import type { CreateSessionParams, CreateSessionResult } from '../../../daemons/session-daemon/shared/SessionTypes';
import { MemoryConsolidationSubprocess } from './modules/cognition/memory/MemoryConsolidationSubprocess';

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

  // Session ID for this PersonaUser (sandboxed identity for tool execution)
  // Generated once during initialization and registered with SessionDaemon
  public sessionId: UUID | null = null;

  // Worker thread for parallel message evaluation
  private worker: PersonaWorkerThread | null = null;

  // AI model configuration (provider, model, temperature, etc.)
  private modelConfig: ModelConfig;

  // Media configuration (opt-in for images/audio/video)
  public mediaConfig: PersonaMediaConfig;

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

  // Task execution module (extracted from PersonaUser for modularity)
  private taskExecutor: PersonaTaskExecutor;

  // Training management module (extracted from PersonaUser for modularity)
  private trainingManager: PersonaTrainingManager;

  // Autonomous servicing loop module (extracted from PersonaUser for modularity)
  private autonomousLoop: PersonaAutonomousLoop;

  // COGNITION SYSTEM: Agent architecture components (memory, reasoning, self-awareness)
  public workingMemory: WorkingMemoryManager;
  public selfState: PersonaSelfState;
  public planFormulator: SimplePlanFormulator;

  // Tool execution adapter (keeps PersonaUser clean)
  private toolExecutor: PersonaToolExecutor;

  // Response generation module (extracted from PersonaUser)
  private responseGenerator: PersonaResponseGenerator;

  // Message evaluation module (extracted from PersonaUser for modularity)
  private messageEvaluator: PersonaMessageEvaluator;

  // Genome management module (extracted from PersonaUser for better genomic daemon integration)
  private genomeManager: PersonaGenomeManager;

  // RTOS-style background subprocess for memory consolidation (optional, non-breaking)
  private memoryWorker?: MemoryConsolidationSubprocess;

  constructor(
    entity: UserEntity,
    state: UserStateEntity,
    storage: IUserStateStorage,
    client?: JTAGClient
  ) {
    super(entity, state, storage, client); // ✅ Pass client to BaseUser for event subscriptions

    // Extract modelConfig from entity (stored via Object.assign during creation)
    // Default to Ollama if not configured
    this.modelConfig = entity.modelConfig || {
      provider: 'ollama',
      model: 'llama3.2:3b',
      temperature: 0.7,
      maxTokens: 150
    };

    // Extract mediaConfig from entity, default to opt-out (no auto-loading)
    // Merge with defaults to ensure all required fields are present
    this.mediaConfig = entity.mediaConfig
      ? { ...DEFAULT_MEDIA_CONFIG, ...entity.mediaConfig }
      : DEFAULT_MEDIA_CONFIG;

    console.log(`🤖 ${this.displayName}: Configured with provider=${this.modelConfig.provider}, model=${this.modelConfig.model}, autoLoadMedia=${this.mediaConfig.autoLoadMedia}`);

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

    // Task execution module (delegated for modularity)
    this.taskExecutor = new PersonaTaskExecutor(this.id, this.displayName, this.memory, this.personaState);

    // Training management module (delegated for modularity)
    this.trainingManager = new PersonaTrainingManager(
      this.id,
      this.displayName,
      this.trainingAccumulator,
      () => this.state,
      () => this.saveState()
    );

    // COGNITION SYSTEM: Agent architecture components
    this.workingMemory = new WorkingMemoryManager(this.id);
    this.selfState = new PersonaSelfState(this.id);
    this.planFormulator = new SimplePlanFormulator(this.id, this.displayName);

    // CNS: Central Nervous System orchestrator (capability-based)
    this.cns = CNSFactory.create(this);

    // Tool execution adapter (dynamic registry for code/read, list, system/daemons, etc.)
    this.toolExecutor = new PersonaToolExecutor(this.id, this.displayName);

    // Response generation module (extracted from PersonaUser for clean separation)
    this.responseGenerator = new PersonaResponseGenerator({
      personaId: this.id,
      personaName: this.displayName,
      entity: this.entity,
      modelConfig: this.modelConfig,
      client,
      toolExecutor: this.toolExecutor,
      mediaConfig: this.mediaConfig,
      getSessionId: () => this.sessionId  // Dynamically get sessionId (set during initialize())
    });

    // Message evaluation module (pass PersonaUser reference for dependency injection)
    this.messageEvaluator = new PersonaMessageEvaluator(this as any); // Cast to match interface

    // Genome management module (delegated for genomic daemon integration)
    this.genomeManager = new PersonaGenomeManager(
      this.id,
      this.displayName,
      () => this.entity,
      () => this.client
    );

    // Autonomous servicing loop module (pass PersonaUser reference for dependency injection)
    this.autonomousLoop = new PersonaAutonomousLoop(this as any); // Cast to match interface

    // RTOS-style memory consolidation subprocess (not started yet - will start in initialize() if enabled)
    // Kept undefined by default to avoid breaking existing behavior
    this.memoryWorker = undefined;

    console.log(`🔧 ${this.displayName}: Initialized inbox, personaState, taskGenerator, memory (genome + RAG), CNS, trainingAccumulator, toolExecutor, responseGenerator, messageEvaluator, autonomousLoop, and cognition system (workingMemory, selfState, planFormulator)`);

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

    // STEP 1.2: Generate sessionId for tool execution attribution (don't register with SessionDaemon yet to avoid init timeout)
    if (!this.sessionId) {
      this.sessionId = generateUUID();
      console.log(`🔐 ${this.displayName}: SessionId generated for tool attribution (${this.sessionId})`);
    }

    // STEP 1.3: Enrich context with callerType='persona' and modelConfig for caller-adaptive command output
    // This enables PersonaUsers to receive media field (base64 image data) from screenshot commands
    // The modelConfig enables commands to resize images based on model's context window capacity
    if (this.client && this.client.context) {
      this.client.context.callerType = 'persona';
      this.client.context.modelConfig = this.modelConfig;
      console.log(`🎯 ${this.displayName}: Context enriched with callerType='persona' and modelConfig for vision-capable tool output`);
    }

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

    // OPTIONAL: Start memory consolidation subprocess (RTOS-style background process)
    // This is opt-in via environment variable to avoid breaking existing behavior
    if (process.env.ENABLE_MEMORY_CONSOLIDATION === 'true') {
      this.memoryWorker = new MemoryConsolidationSubprocess(this as any);
      await this.memoryWorker.start();
      console.log(`🧠 ${this.displayName}: Memory consolidation subprocess started`);
    }
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
   * Evaluate message and possibly respond WITH COGNITION (called with exclusive evaluation lock)
   *
   * NEW: Wraps existing chat logic with plan-based reasoning:
   * 1. Create Task from message
   * 2. Generate Plan using SimplePlanFormulator
   * 3. Execute plan steps (existing chat logic runs inside)
   * 4. Store thoughts in WorkingMemory
   * 5. Update SelfState with focus and cognitive load
   */
  private async evaluateAndPossiblyRespondWithCognition(
    messageEntity: ChatMessageEntity,
    senderIsHuman: boolean,
    messageText: string
  ): Promise<void> {
    return await this.messageEvaluator.evaluateAndPossiblyRespondWithCognition(messageEntity, senderIsHuman, messageText);
  }

  /**
   * Evaluate message and possibly respond (called with exclusive evaluation lock)
   *
   * NOTE: Now called from evaluateAndPossiblyRespondWithCognition wrapper
   */
  private async evaluateAndPossiblyRespond(
    messageEntity: ChatMessageEntity,
    senderIsHuman: boolean,
    messageText: string
  ): Promise<void> {
    return await this.messageEvaluator.evaluateAndPossiblyRespond(messageEntity, senderIsHuman, messageText);
  }

  // Response generation core logic moved to PersonaResponseGenerator module
  // But some utility methods still needed by other parts of PersonaUser:

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
   * PUBLIC: Used by PersonaMessageEvaluator module
   */
  timestampToNumber(timestamp: Date | number | undefined): number {
    if (timestamp === undefined) {
      return Date.now(); // Use current time if timestamp missing
    }
    return timestamp instanceof Date ? timestamp.getTime() : timestamp;
  }

  // Tool execution methods delegated to PersonaToolExecutor adapter

  /**
   * Generate and post a response to a chat message
   * Phase 2: AI-powered responses with RAG context via AIProviderDaemon
   *
   * DELEGATED to PersonaResponseGenerator module (extracted for clean separation)
   *
   * **Dormancy filtering**: Checks dormancy state before responding
   */
  private async respondToMessage(
    originalMessage: ChatMessageEntity,
    decisionContext?: Omit<LogDecisionParams, 'responseContent' | 'tokensUsed' | 'responseTime'>
  ): Promise<void> {
    // Check dormancy state before responding
    const shouldRespond = this.responseGenerator.shouldRespondToMessage(
      originalMessage,
      this.state.dormancyState
    );

    if (!shouldRespond) {
      // Dormancy filtered - skip response
      return;
    }

    await this.responseGenerator.generateAndPostResponse(originalMessage, decisionContext);
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
        timestamp: Date.now(),
        // Pass PersonaState for smarter evaluation
        personaState: {
          energy: this.state.energy,
          attention: this.state.attention,
          mood: this.state.mood,
          inboxLoad: this.state.inboxLoad
        },
        // Pass config for threshold/temperature
        config: {
          responseThreshold: this.entity?.personaConfig?.responseThreshold ?? 50,
          temperature: this.entity?.modelConfig?.temperature ?? 0.7
        }
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
   *
   * DELEGATED TO: PersonaGenomeManager.getGenome()
   */
  async getGenome(): Promise<GenomeEntity | null> {
    return await this.genomeManager.getGenome();
  }

  /**
   * Set genome for this persona (Phase 1.2)
   * Updates the genomeId field and persists to database
   *
   * DELEGATED TO: PersonaGenomeManager.setGenome()
   */
  async setGenome(genomeId: UUID): Promise<boolean> {
    return await this.genomeManager.setGenome(genomeId);
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
    return await this.messageEvaluator.evaluateShouldRespond(message, senderIsHuman, isMentioned);
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
    this.autonomousLoop.startAutonomousServicing();
  }


  /**
   * CNS callback: Poll tasks from database
   *
   * Called by PersonaCentralNervousSystem.serviceCycle() via callback pattern.
   */
  public async pollTasksFromCNS(): Promise<void> {
    await this.autonomousLoop.pollTasksFromCNS();
  }

  /**
   * CNS callback: Generate self-tasks for autonomous work
   *
   * Called by PersonaCentralNervousSystem.serviceCycle() via callback pattern.
   */
  public async generateSelfTasksFromCNS(): Promise<void> {
    await this.autonomousLoop.generateSelfTasksFromCNS();
  }

  /**
   * CNS callback: Handle chat message from CNS orchestrator
   *
   * This is called by PersonaCentralNervousSystem.serviceChatDomain() via callback pattern.
   * Preserves existing message handling logic (evaluation, RAG, AI response, posting).
   */
  public async handleChatMessageFromCNS(item: QueueItem): Promise<void> {
    await this.autonomousLoop.handleChatMessageFromCNS(item);
  }

  /**
   * PHASE 5: Execute a task based on its type
   *
   * Handles all task types: memory-consolidation, skill-audit, fine-tune-lora, resume-work, etc.
   * Delegates to PersonaTaskExecutor module for actual execution.
   */
  private async executeTask(task: InboxTask): Promise<void> {
    // Delegate to task executor module
    await this.taskExecutor.executeTask(task);
  }

  /**
   * Shutdown worker thread and cleanup resources
   */
  async shutdown(): Promise<void> {
    // Stop memory consolidation subprocess if running
    if (this.memoryWorker) {
      await this.memoryWorker.stop();
      console.log(`🧠 ${this.displayName}: Memory consolidation subprocess stopped`);
    }

    // Stop autonomous servicing loop
    await this.autonomousLoop.stopServicing();

    // PHASE 6: Shutdown memory module (genome + RAG)
    await this.memory.shutdown();

    if (this.worker) {
      await this.worker.shutdown();
      console.log(`🧵 ${this.displayName}: Worker thread shut down`);
      this.worker = null;
    }
  }

}
