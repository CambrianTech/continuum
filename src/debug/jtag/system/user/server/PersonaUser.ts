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
import { PersonaToolRegistry } from './modules/PersonaToolRegistry';
import { PersonaTaskExecutor } from './modules/PersonaTaskExecutor';
import { PersonaTrainingManager } from './modules/PersonaTrainingManager';
import { PersonaAutonomousLoop } from './modules/PersonaAutonomousLoop';
import { PersonaResponseGenerator } from './modules/PersonaResponseGenerator';
import { PersonaMessageEvaluator } from './modules/PersonaMessageEvaluator';
import { PersonaTaskTracker } from './modules/PersonaTaskTracker';
import { PersonaGenomeManager } from './modules/PersonaGenomeManager';
import { type PersonaMediaConfig, DEFAULT_MEDIA_CONFIG } from './modules/PersonaMediaConfig';
import type { CreateSessionParams, CreateSessionResult } from '../../../daemons/session-daemon/shared/SessionTypes';
import { Hippocampus } from './modules/cognitive/memory/Hippocampus';
import { PersonaLogger } from './modules/PersonaLogger';
import { PersonaSoul, type PersonaUserForSoul } from './modules/being/PersonaSoul';
import { PersonaMind, type PersonaUserForMind } from './modules/being/PersonaMind';
import { PersonaBody, type PersonaUserForBody } from './modules/being/PersonaBody';
import { SystemPaths } from '../../core/config/SystemPaths';

/**
 * PersonaUser - Our internal AI citizens
 *
 * First-class citizens with their own JTAGClient for universal Commands/Events API
 */
export class PersonaUser extends AIUser {
  /**
   * Implementation of abstract homeDirectory getter from BaseUser
   * PersonaUsers live in the 'personas/' directory
   * Returns ABSOLUTE path via SystemPaths - THE SINGLE SOURCE OF TRUTH
   */
  get homeDirectory(): string {
    return SystemPaths.personas.dir(this.entity.uniqueId);
  }

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
  public modelConfig: ModelConfig;

  // Media configuration (opt-in for images/audio/video)
  public mediaConfig: PersonaMediaConfig;

  // Rate limiting module (TODO: Replace with AI-based coordination when ThoughtStream is solid)
  readonly rateLimiter: RateLimiter;

  // PHASE 1: Autonomous servicing modules (inbox + personaState)
  // Inbox stores messages with priority, personaState tracks energy/mood
  // NOTE: Can't name this 'state' - conflicts with BaseUser.state (UserStateEntity)
  public inbox: PersonaInbox;

  // BEING ARCHITECTURE: Delegate to mind for personaState
  public get personaState(): PersonaStateManager {
    if (!this.mind) throw new Error('Mind not initialized');
    return this.mind.personaState;
  }

  // PHASE 5: Self-task generation (autonomous work creation)
  readonly taskGenerator: SelfTaskGenerator;

  // Tool result tracking (prevents infinite loops from re-processing tool results)
  readonly taskTracker: PersonaTaskTracker;

  // BEING ARCHITECTURE: Soul system (memory, learning, identity)
  private soul: PersonaSoul | null = null;

  // BEING ARCHITECTURE: Mind system (cognition, evaluation, planning)
  public mind: PersonaMind | null = null;  // Public for CNS and Hippocampus access

  // BEING ARCHITECTURE: Body system (action, execution, output)
  private body: PersonaBody | null = null;

  // BEING ARCHITECTURE: Delegate to soul for memory/genome/training/hippocampus
  public get memory(): PersonaMemory {
    if (!this.soul) throw new Error('Soul not initialized');
    return this.soul.memory;
  }

  public get trainingAccumulator(): TrainingDataAccumulator {
    if (!this.soul) throw new Error('Soul not initialized');
    return this.soul.trainingAccumulator;
  }

  private get genomeManager(): PersonaGenomeManager {
    if (!this.soul) throw new Error('Soul not initialized');
    return this.soul.genomeManager;
  }

  private get hippocampus(): Hippocampus {
    if (!this.soul) throw new Error('Soul not initialized');
    return this.soul.hippocampus;
  }

  public get trainingManager(): PersonaTrainingManager {
    if (!this.soul) throw new Error('Soul not initialized');
    return this.soul.trainingManager;
  }

  // PHASE 6: Decision Adapter Chain (fast-path, thermal, LLM gating)
  readonly decisionChain: DecisionAdapterChain;

  // CNS: Central Nervous System orchestrator
  readonly cns: PersonaCentralNervousSystem;

  // Task execution module (extracted from PersonaUser for modularity)
  readonly taskExecutor: PersonaTaskExecutor;

  // Autonomous servicing loop module (extracted from PersonaUser for modularity)
  private autonomousLoop: PersonaAutonomousLoop;

  // BEING ARCHITECTURE: Delegate to mind for workingMemory
  public get workingMemory(): WorkingMemoryManager {
    if (!this.mind) throw new Error('Mind not initialized');
    return this.mind.workingMemory;
  }

  // BEING ARCHITECTURE: Delegate to mind for selfState
  public get selfState(): PersonaSelfState {
    if (!this.mind) throw new Error('Mind not initialized');
    return this.mind.selfState;
  }

  // BEING ARCHITECTURE: Delegate to mind for planFormulator
  public get planFormulator(): SimplePlanFormulator {
    if (!this.mind) throw new Error('Mind not initialized');
    return this.mind.planFormulator;
  }

  // BEING ARCHITECTURE: Delegate to body for toolExecutor
  private get toolExecutor(): PersonaToolExecutor {
    if (!this.body) throw new Error('Body not initialized');
    return this.body.toolExecutor;
  }

  // BEING ARCHITECTURE: Delegate to body for toolRegistry
  private get toolRegistry(): PersonaToolRegistry {
    if (!this.body) throw new Error('Body not initialized');
    return this.body.toolRegistry;
  }

  // BEING ARCHITECTURE: Delegate to body for responseGenerator
  private get responseGenerator(): PersonaResponseGenerator {
    if (!this.body) throw new Error('Body not initialized');
    return this.body.responseGenerator;
  }

  // Message evaluation module (extracted from PersonaUser for modularity)
  private messageEvaluator: PersonaMessageEvaluator;

  // RTOS Subprocesses
  public logger: PersonaLogger; // Public: accessed by all subprocesses for logging
  // Note: genomeManager and hippocampus now accessed via soul getters

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

    this.log.info(`🤖 ${this.displayName}: Configured with provider=${this.modelConfig.provider}, model=${this.modelConfig.model}, autoLoadMedia=${this.mediaConfig.autoLoadMedia}`);

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
    // Inject logger for cognition.log
    this.inbox.setLogger((message: string) => {
      this.logger.enqueueLog('cognition.log', message);
    });
    // Inject queue stats provider for load-aware deduplication (feedback loop)
    this.inbox.setQueueStatsProvider(() => {
      const adapter = AIProviderDaemon.getAdapter('ollama');
      if (adapter && adapter.getQueueStats) {
        return adapter.getQueueStats();
      }
      return { queueSize: 0, activeRequests: 0, maxConcurrent: 1, load: 0.0 };
    });

    // PHASE 5: Self-task generation for autonomous work creation
    this.taskGenerator = new SelfTaskGenerator(this.id, this.displayName, {
      enabled: true,  // Enable self-task generation
      memoryReviewInterval: 3600000,      // 1 hour
      skillAuditInterval: 21600000,       // 6 hours
      unfinishedWorkThreshold: 1800000    // 30 minutes
    });

    // Tool result tracking (prevents infinite response loops)
    this.taskTracker = new PersonaTaskTracker();

    // Initialize logger FIRST - other subsystems need it
    this.logger = new PersonaLogger(this);

    // BEING ARCHITECTURE Phase 1: Initialize Soul FIRST (memory, learning, identity)
    // Soul wraps memory/genome/learning systems - must be initialized before anything that uses getters
    this.soul = new PersonaSoul(this as any as PersonaUserForSoul);

    // BEING ARCHITECTURE Phase 2: Initialize Mind (cognition, evaluation, planning)
    this.mind = new PersonaMind(this as any as PersonaUserForMind);

    // BEING ARCHITECTURE Phase 3: Initialize Body (action, execution, output)
    // Note: Body creates toolExecutor, toolRegistry, and responseGenerator internally
    this.body = new PersonaBody({
      id: this.id,
      displayName: this.displayName,
      entity: this.entity,
      modelConfig: this.modelConfig,
      client,
      mediaConfig: this.mediaConfig,
      getSessionId: () => this.sessionId,
      homeDirectory: this.homeDirectory,
      logger: this.logger
    });

    // PHASE 6: Decision adapter chain (fast-path, thermal, LLM gating)
    // Pass logger for cognition.log
    const cognitionLogger = (message: string, ...args: any[]) => {
      this.logger.enqueueLog('cognition.log', message);
    };
    this.decisionChain = new DecisionAdapterChain(cognitionLogger);
    this.log.info(`🔗 ${this.displayName}: Decision adapter chain initialized with ${this.decisionChain.getAllAdapters().length} adapters`);

    // Task execution module (delegated for modularity, uses this.memory getter)
    this.taskExecutor = new PersonaTaskExecutor(this.id, this.displayName, this.memory, this.personaState, cognitionLogger);

    // CNS: Central Nervous System orchestrator (capability-based)
    // Note: mind/soul/body are non-null at this point (initialized above)
    this.cns = CNSFactory.create(this);

    // Message evaluation module (pass PersonaUser reference for dependency injection)
    this.messageEvaluator = new PersonaMessageEvaluator(this);

    // Autonomous servicing loop module (pass PersonaUser reference for dependency injection)
    this.autonomousLoop = new PersonaAutonomousLoop(this, cognitionLogger);

    this.log.info(`🔧 ${this.displayName}: Initialized inbox, personaState, taskGenerator, memory (genome + RAG), CNS, trainingAccumulator, toolExecutor, responseGenerator, messageEvaluator, autonomousLoop, and cognition system (workingMemory, selfState, planFormulator)`);

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
  public logAIDecision(
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
      this.log.debug(`🔐 ${this.displayName}: SessionId generated for tool attribution (${this.sessionId})`);
    }

    // STEP 1.3: Enrich context with callerType='persona' and modelConfig for caller-adaptive command output
    // This enables PersonaUsers to receive media field (base64 image data) from screenshot commands
    // The modelConfig enables commands to resize images based on model's context window capacity
    if (this.client && this.client.context) {
      this.client.context.callerType = 'persona';
      this.client.context.modelConfig = this.modelConfig;
      this.log.debug(`🎯 ${this.displayName}: Context enriched with callerType='persona' and modelConfig for vision-capable tool output`);
    }

    // STEP 1.5: Start worker thread for message evaluation
    if (this.worker) {
      await this.worker.start();
      this.log.info(`🧵 ${this.displayName}: Worker thread started`);
    }

    // STEP 1.6: Register with ResourceManager for holistic resource allocation
    try {
      const { getResourceManager } = await import('../../resources/shared/ResourceManager.js');
      getResourceManager().registerAdapter(this.id, this.displayName);
      this.log.info(`🔧 ${this.displayName}: Registered with ResourceManager`);
    } catch (error) {
      this.log.warn(`⚠️  ${this.displayName}: Could not register with ResourceManager:`, error);
      // Non-fatal: isAvailable() will default to simple worker ready check
    }

    // STEP 2: Subscribe to room-specific chat events (only if client available)
    if (this.client && !this.eventsSubscribed) {
      this.log.debug(`🔧 ${this.displayName}: About to subscribe to ${this.myRoomIds.size} room(s), eventsSubscribed=${this.eventsSubscribed}`);

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
      this.log.info(`✅ ${this.displayName}: Subscriptions complete, eventsSubscribed=${this.eventsSubscribed}`);
    } else {
      this.log.info(`⏭️ ${this.displayName}: Skipping subscriptions (already subscribed or no client), eventsSubscribed=${this.eventsSubscribed}, hasClient=${!!this.client}`);
    }

    this.isInitialized = true;

    // PHASE 3: Start autonomous servicing loop (lifecycle-based)
    this.startAutonomousServicing();

    // Start RTOS subprocesses
    // Logger MUST start first - other subprocesses depend on it for logging
    await this.logger.start();
    this.log.info(`📝 ${this.displayName}: PersonaLogger started (queued, non-blocking logging)`);

    // Start soul memory consolidation (Hippocampus subprocess via soul interface)
    await this.soul!.startMemoryConsolidation();
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
      this.log.warn(`⚠️ ${this.displayName}: Cannot auto-join general room - no client available`);
      return;
    }

    try {
      // Query for general room using DataDaemon.query (server-side only)
      const queryResult = await DataDaemon.query<RoomEntity>({
        collection: COLLECTIONS.ROOMS,
        filter: { uniqueId: ROOM_UNIQUE_IDS.GENERAL }
      });

      if (!queryResult.success || !queryResult.data?.length) {
        this.log.warn(`⚠️ ${this.displayName}: General room not found - cannot auto-join`);
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
        this.log.debug(`✅ ${this.displayName}: Already member of general room`);
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

      this.log.info(`✅ ${this.displayName}: Auto-joined general room (added to members array)`);
      // Reload my rooms to pick up the change
      await this.loadMyRooms();
    } catch (error) {
      this.log.error(`❌ ${this.displayName}: Error auto-joining general room:`, error);
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
      this.log.debug(`⏭️ ${this.displayName}: Skipping resolved message from ${messageEntity.senderName}`);
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

    this.log.info(`📨 ${this.displayName}: Enqueued message (priority=${priority.toFixed(2)}, inbox size=${this.inbox.getSize()}, mood=${this.personaState.getState().mood})`);

    // PHASE 3: Autonomous polling loop will service inbox at adaptive cadence
    // (No immediate processing - messages wait in inbox until loop polls)
    // NOTE: Memory creation handled autonomously by Hippocampus subprocess
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
  public async evaluateAndPossiblyRespondWithCognition(
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
  public async respondToMessage(
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

    const result = await this.responseGenerator.generateAndPostResponse(originalMessage, decisionContext);

    // Mark tool results as processed to prevent infinite loops
    if (result.success && result.storedToolResultIds.length > 0) {
      this.taskTracker.markMultipleProcessed(result.storedToolResultIds);
    }
  }

  /**
   * Generate text using this persona's LLM
   *
   * This is THE interface for all LLM inference:
   * - Chat responses (via PersonaResponseGenerator)
   * - Memory synthesis (via Hippocampus adapters)
   * - Task generation (future)
   * - Self-reflection (future)
   *
   * All inference goes through here. Uses same provider/model/timeout as chat.
   */
  public async generateText(request: {
    prompt: string;
    temperature?: number;
    maxTokens?: number;
    systemPrompt?: string;
    context?: string;  // For logging/metrics (e.g., 'memory-synthesis', 'task-generation')
  }): Promise<string> {
    try {
      const messages: { role: 'system' | 'user'; content: string }[] = [];

      if (request.systemPrompt) {
        messages.push({
          role: 'system',
          content: request.systemPrompt
        });
      }

      messages.push({
        role: 'user',
        content: request.prompt
      });

      const genRequest: TextGenerationRequest = {
        messages,
        model: this.modelConfig.model || 'llama3.2:3b',
        temperature: request.temperature ?? this.modelConfig.temperature ?? 0.7,
        maxTokens: request.maxTokens ?? this.modelConfig.maxTokens ?? 150,
        preferredProvider: (this.modelConfig.provider || 'ollama') as TextGenerationRequest['preferredProvider'],
        intelligenceLevel: this.entity.intelligenceLevel,
        personaContext: {
          uniqueId: this.entity.uniqueId,
          displayName: this.displayName,
          logDir: SystemPaths.personas.dir(this.entity.uniqueId)
        }
      };

      // Use same 180s timeout as chat responses
      const GENERATION_TIMEOUT_MS = 180000;
      const timeoutPromise = new Promise<never>((_, reject) => {
        setTimeout(() => reject(new Error(`AI generation timeout after 180 seconds (context: ${request.context || 'unknown'})`)), GENERATION_TIMEOUT_MS);
      });

      const response = await Promise.race([
        AIProviderDaemon.generateText(genRequest),
        timeoutPromise
      ]);

      return response.text;
    } catch (error) {
      this.log.error(`❌ ${this.displayName}: Text generation failed (context=${request.context || 'unknown'}): ${error}`);
      throw error;
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
      this.log.debug(`🔇 ${this.displayName}: Requires explicit mention but wasn't mentioned - staying silent`);
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

      this.log.debug(`🧵 ${this.displayName}: Worker evaluated message ${messageEntity.id} - rawConfidence=${result.confidence.toFixed(2)}, agePenalty=${agePenalty.toFixed(2)} (${messageAgeMinutes.toFixed(1)}min old), adjustedConfidence=${adjustedConfidence.toFixed(2)}, threshold=${threshold.toFixed(2)}, shouldRespond=${shouldRespond}`);

      return shouldRespond;

    } catch (error) {
      this.log.error(`❌ ${this.displayName}: Fast gating failed, falling back to heuristics:`, error);

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
      this.log.warn(`⚠️  PersonaUser ${this.displayName}: Cannot check sender type - no client, BLOCKING response`);
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
        this.log.warn(`⚠️  PersonaUser ${this.displayName}: Could not read sender ${senderId}, BLOCKING response`);
        return false; // Fail CLOSED - don't respond if database fails (prevents loops)
      }

      const senderType = result.data.type;
      return senderType === 'human';

    } catch (error) {
      this.log.error(`❌ PersonaUser ${this.displayName}: Error checking sender type, BLOCKING response:`, error);
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
    // Stop soul systems (hippocampus + memory consolidation)
    await this.soul!.shutdown();

    // Force flush all queued logs before stopping logger
    await this.logger.forceFlush();

    // Stop logger last (ensure all logs written)
    await this.logger.stop();
    this.log.info(`📝 ${this.displayName}: PersonaLogger stopped (all logs flushed)`);

    // Stop autonomous servicing loop
    await this.autonomousLoop.stopServicing();

    // PHASE 6: Shutdown memory module (genome + RAG)
    await this.memory.shutdown();

    if (this.worker) {
      await this.worker.shutdown();
      this.log.info(`🧵 ${this.displayName}: Worker thread shut down`);
      this.worker = null;
    }
  }

}
