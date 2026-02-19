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
import type { UserCreateParams } from '../../../commands/user/create/shared/UserCreateTypes';
import type { ModelConfig } from '../../data/entities/UserEntity';
import type { DataCreateParams, DataCreateResult } from '../../../commands/data/create/shared/DataCreateTypes';
import type { DataReadParams, DataReadResult } from '../../../commands/data/read/shared/DataReadTypes';
import type { DataUpdateParams, DataUpdateResult } from '../../../commands/data/update/shared/DataUpdateTypes';
import type { Thought, ThoughtType } from '../../conversation/shared/ConversationCoordinationTypes';
import { getChatCoordinator, type ChatThought } from '../../coordination/server/ChatCoordinationStream';
import { MemoryStateBackend } from '../storage/MemoryStateBackend';
import { getDefaultCapabilitiesForType, getDefaultPreferencesForType } from '../config/UserCapabilitiesDefaults';
import { ORM } from '../../../daemons/data-daemon/server/ORM';
import { COLLECTIONS } from '../../data/config/DatabaseConfig';
import { getDataEventName } from '../../core/shared/EventConstants';
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
import type { RAGContext as PipelineRAGContext } from '../../rag/shared/RAGTypes';
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
import { CodeDaemon } from '../../../daemons/code-daemon/shared/CodeDaemon';
import { ROOM_UNIQUE_IDS } from '../../data/constants/RoomConstants';
import { DataList, type DataListParams, type DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import type { StageCompleteEvent } from '../../conversation/shared/CognitionEventTypes';
import { calculateSpeedScore, getStageStatus, COGNITION_EVENTS } from '../../conversation/shared/CognitionEventTypes';
import { RateLimiter } from './modules/RateLimiter';
import { PersonaInbox, calculateMessagePriority } from './modules/PersonaInbox';
import { PersonaStateManager } from './modules/PersonaState';
import type { InboxMessage } from './modules/PersonaInbox';
import type { InboxTask, TaskStatus, ProcessableMessage } from './modules/QueueItemTypes';
import { TrainingDataAccumulator } from './modules/TrainingDataAccumulator';
import { PersonaGenome, type PersonaGenomeConfig } from './modules/PersonaGenome';
import type { QueueItem } from './modules/PersonaInbox';
import type { FastPathDecision } from './modules/central-nervous-system/CNSTypes';
import { PersonaMemory } from './modules/cognitive/memory/PersonaMemory';
// NOTE: DecisionAdapterChain removed - Rust cognition engine handles fast-path decisions
// See: workers/continuum-core/src/persona/cognition.rs
import type { DecisionContext } from './modules/cognition/adapters/IDecisionAdapter';
import { WorkingMemoryManager } from './modules/cognition/memory/WorkingMemoryManager';
import { PersonaSelfState } from './modules/cognition/PersonaSelfState';
import { SimplePlanFormulator } from './modules/cognition/reasoning/SimplePlanFormulator';
import type { Task, Plan } from './modules/cognition/reasoning/types';
import { CognitionLogger } from './modules/cognition/CognitionLogger';
import { PersonaToolExecutor } from './modules/PersonaToolExecutor';
import { PersonaToolRegistry } from './modules/PersonaToolRegistry';
import { PersonaTaskExecutor } from './modules/PersonaTaskExecutor';
import { LOCAL_MODELS } from '../../shared/Constants';
import { PersonaTrainingManager } from './modules/PersonaTrainingManager';
import { PersonaAutonomousLoop } from './modules/PersonaAutonomousLoop';
import { PersonaResponseGenerator } from './modules/PersonaResponseGenerator';
import { TimingHarness } from '../../core/shared/TimingHarness';
import { PersonaMessageEvaluator } from './modules/PersonaMessageEvaluator';
import { PersonaTaskTracker } from './modules/PersonaTaskTracker';
import { PersonaGenomeManager } from './modules/PersonaGenomeManager';
import { type PersonaMediaConfig, DEFAULT_MEDIA_CONFIG } from './modules/PersonaMediaConfig';
import type { CreateSessionParams, CreateSessionResult } from '../../../daemons/session-daemon/shared/SessionTypes';
import { Hippocampus } from './modules/cognitive/memory/Hippocampus';
import type { RecallParams, MemoryEntity } from './modules/MemoryTypes';
import { PersonaLogger } from './modules/PersonaLogger';
import { setToolDefinitionsLogger } from './modules/PersonaToolDefinitions';
import { setPeerReviewLogger } from './modules/cognition/PeerReviewManager';
import { LimbicSystem, type PersonaUserForLimbic } from './modules/being/LimbicSystem';
import { PrefrontalCortex, type PersonaUserForPrefrontal } from './modules/being/PrefrontalCortex';
import { MotorCortex, type PersonaUserForMotorCortex } from './modules/being/MotorCortex';
import { RustCognitionBridge, type PersonaUserForRustCognition } from './modules/RustCognitionBridge';
import { SystemPaths } from '../../core/config/SystemPaths';
import { UnifiedConsciousness } from './modules/consciousness/UnifiedConsciousness';
import { registerConsciousness, unregisterConsciousness } from '../../rag/sources/GlobalAwarenessSource';
import { Workspace } from '../../code/server/Workspace';
import { initShellEventHandler } from './modules/ShellEventHandler';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { DataOpen } from '../../../commands/data/open/shared/DataOpenTypes';
import type { CorpusMemory } from '../../../workers/continuum-core/bindings/CorpusMemory';
import type { CorpusTimelineEvent } from '../../../workers/continuum-core/bindings/CorpusTimelineEvent';

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

  // NEUROANATOMY: Delegate to prefrontal for personaState
  public get personaState(): PersonaStateManager {
    if (!this.prefrontal) throw new Error('Prefrontal cortex not initialized');
    return this.prefrontal.personaState;
  }

  // PHASE 5: Self-task generation (autonomous work creation)
  // taskGenerator removed — self-task generation now runs in Rust (ChannelModule.tick())

  // Tool result tracking (prevents infinite loops from re-processing tool results)
  readonly taskTracker: PersonaTaskTracker;

  // NEUROANATOMY: Limbic system (memory, learning, identity, emotion)
  private limbic: LimbicSystem | null = null;

  // NEUROANATOMY: Prefrontal cortex (cognition, evaluation, planning)
  public prefrontal: PrefrontalCortex | null = null;  // Public for Hippocampus access

  // NEUROANATOMY: Motor cortex (action, execution, output)
  private motorCortex: MotorCortex | null = null;

  // RUST COGNITION: Fast-path decision engine via IPC (<1ms)
  // Handles priority calculation, deduplication, state-based gating in Rust
  private _rustCognition: RustCognitionBridge | null = null;

  // UNIFIED CONSCIOUSNESS: Cross-context awareness layer (no severance!)
  // Sits ABOVE limbic/prefrontal - provides global timeline, intentions, peripheral awareness
  private _consciousness: UnifiedConsciousness | null = null;

  // Room name cache for contextName in timeline events
  private _roomNameCache: Map<UUID, string> = new Map();

  // MEMORY LEAK FIX: Track event subscriptions for cleanup
  private _eventUnsubscribes: (() => void)[] = [];

  // Workspace handles — lazy-created per context key, retained for session lifetime
  // Keyed by context (e.g., room uniqueId) so personas can have per-room workspaces
  private _workspaces: Map<string, Workspace> = new Map();

  /**
   * Get unified consciousness for cross-context awareness
   * Public for RAG sources and cognitive modules
   */
  public get consciousness(): UnifiedConsciousness {
    if (!this._consciousness) throw new Error('Consciousness not initialized');
    return this._consciousness;
  }

  /**
   * Get Rust cognition bridge for fast-path decisions
   * Public for modules that need sub-1ms priority calculation or should-respond decisions
   */
  public get rustCognition(): RustCognitionBridge {
    if (!this._rustCognition) throw new Error('Rust cognition bridge not initialized');
    return this._rustCognition;
  }

  /**
   * Nullable accessor for Rust bridge (used during construction before bridge is ready).
   * Unlike rustCognition getter, this returns null instead of throwing.
   */
  public get rustCognitionBridge(): RustCognitionBridge | null {
    return this._rustCognition;
  }

  // NEUROANATOMY: Delegate to limbic for memory/genome/training/hippocampus
  public get memory(): PersonaMemory {
    if (!this.limbic) throw new Error('Limbic system not initialized');
    return this.limbic.memory;
  }

  public get trainingAccumulator(): TrainingDataAccumulator {
    if (!this.limbic) throw new Error('Limbic system not initialized');
    return this.limbic.trainingAccumulator;
  }

  private get genomeManager(): PersonaGenomeManager {
    if (!this.limbic) throw new Error('Limbic system not initialized');
    return this.limbic.genomeManager;
  }

  private get hippocampus(): Hippocampus {
    if (!this.limbic) throw new Error('Limbic system not initialized');
    return this.limbic.hippocampus;
  }

  /**
   * Recall memories from long-term memory (Hippocampus)
   * Public interface for RAG and other systems to access consolidated memories
   */
  public async recallMemories(params: RecallParams): Promise<MemoryEntity[]> {
    if (!this.limbic) {
      return [];
    }
    return this.hippocampus.recall(params);
  }

  /**
   * Semantic recall - query memories by meaning, not just filters
   * Uses vector similarity search for semantically relevant memories
   *
   * @param queryText - Natural language query (e.g., recent message content)
   * @param params - Additional filter constraints
   */
  public async semanticRecallMemories(queryText: string, params: RecallParams = {}): Promise<MemoryEntity[]> {
    if (!this.limbic) {
      return [];
    }
    return this.hippocampus.semanticRecall(queryText, params);
  }

  public get trainingManager(): PersonaTrainingManager {
    if (!this.limbic) throw new Error('Limbic system not initialized');
    return this.limbic.trainingManager;
  }

  // NOTE: DecisionAdapterChain removed - Rust cognition handles fast-path decisions
  // See: workers/continuum-core/src/persona/cognition.rs (PersonaCognitionEngine)

  // CNS removed — scheduling inlined into PersonaAutonomousLoop (service loop calls Rust directly)

  // Task execution module (extracted from PersonaUser for modularity)
  readonly taskExecutor: PersonaTaskExecutor;

  // Autonomous servicing loop module (extracted from PersonaUser for modularity)
  private autonomousLoop: PersonaAutonomousLoop;

  // BEING ARCHITECTURE: Delegate to mind for workingMemory
  public get workingMemory(): WorkingMemoryManager {
    if (!this.prefrontal) throw new Error('Prefrontal cortex not initialized');
    return this.prefrontal.workingMemory;
  }

  // BEING ARCHITECTURE: Delegate to mind for selfState
  public get selfState(): PersonaSelfState {
    if (!this.prefrontal) throw new Error('Prefrontal cortex not initialized');
    return this.prefrontal.selfState;
  }

  // BEING ARCHITECTURE: Delegate to mind for planFormulator
  public get planFormulator(): SimplePlanFormulator {
    if (!this.prefrontal) throw new Error('Prefrontal cortex not initialized');
    return this.prefrontal.planFormulator;
  }

  // ════════════════════════════════════════════════════════════════════════════
  // Workspace — per-persona code workspace (lazy-created, session-scoped)
  // ════════════════════════════════════════════════════════════════════════════

  /** Get a workspace by context key (null if not yet created for that context) */
  public getWorkspace(contextKey: string = 'default'): Workspace | null {
    return this._workspaces.get(contextKey) ?? null;
  }

  /**
   * Ensure a workspace exists for this persona in the given context.
   * Creates on first call per context key, retains for session lifetime.
   * Called automatically when persona receives a code-domain task.
   *
   * @param options.contextKey  Room uniqueId or other scope key (default: 'default')
   * @param options.mode        'sandbox' for isolated, 'worktree' for real git branches
   * @param options.taskSlug    Used for branch naming in worktree mode
   * @param options.sparsePaths Sparse checkout paths for worktree mode
   */
  public async ensureWorkspace(options?: {
    contextKey?: string;
    mode?: 'sandbox' | 'worktree' | 'project';
    taskSlug?: string;
    sparsePaths?: string[];
    repoPath?: string;
  }): Promise<Workspace> {
    const key = options?.contextKey ?? 'default';
    const existing = this._workspaces.get(key);
    if (existing) return existing;

    const mode = options?.mode ?? 'sandbox';
    this.log.info(`${this.displayName}: Creating workspace (${mode} mode, context=${key})`);
    const ws = await Workspace.create({
      personaId: this.id,
      mode,
      taskSlug: options?.taskSlug ?? key,
      sparsePaths: options?.sparsePaths,
      repoPath: options?.repoPath,
      personaName: this.displayName,
      personaUniqueId: this.entity.uniqueId,
    });
    this._workspaces.set(key, ws);
    this.log.info(`${this.displayName}: Workspace created — handle=${ws.handle}, dir=${ws.dir}, mode=${mode}${ws.branch ? `, branch=${ws.branch}` : ''}`);
    return ws;
  }

  // BEING ARCHITECTURE: Delegate to body for toolExecutor
  private get toolExecutor(): PersonaToolExecutor {
    if (!this.motorCortex) throw new Error('Motor cortex not initialized');
    return this.motorCortex.toolExecutor;
  }

  // BEING ARCHITECTURE: Delegate to body for toolRegistry
  private get toolRegistry(): PersonaToolRegistry {
    if (!this.motorCortex) throw new Error('Motor cortex not initialized');
    return this.motorCortex.toolRegistry;
  }

  // BEING ARCHITECTURE: Delegate to body for responseGenerator
  private get responseGenerator(): PersonaResponseGenerator {
    if (!this.motorCortex) throw new Error('Motor cortex not initialized');
    return this.motorCortex.responseGenerator;
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

    // PersonaUser MUST have a provider — it's an AI, not a human.
    // Model ID comes from getModelConfigForProvider() defaults if not set on entity.
    if (!entity.modelConfig?.provider) {
      throw new Error(
        `PersonaUser '${entity.displayName}' missing required modelConfig.provider. ` +
        `Every persona must have provider set in seed data.`
      );
    }
    // Provider defaults fill in model, temperature, maxTokens, systemPrompt etc.
    // Entity's explicit values override defaults.
    const providerDefaults = getModelConfigForProvider(entity.modelConfig.provider);
    this.modelConfig = {
      ...providerDefaults,
      ...entity.modelConfig
    };
    // Validate the MERGED result has both model and provider
    if (!this.modelConfig.model || !this.modelConfig.provider) {
      throw new Error(
        `PersonaUser '${entity.displayName}' modelConfig incomplete after merge with provider defaults. ` +
        `model=${this.modelConfig.model}, provider=${this.modelConfig.provider}. ` +
        `Check DEFAULT_MODEL_CONFIGS for provider '${entity.modelConfig.provider}'.`
      );
    }

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
    // CRITICAL: Handle case where AIProviderDaemon isn't initialized yet (race condition on startup)
    this.inbox.setQueueStatsProvider(() => {
      try {
        const adapter = AIProviderDaemon.getAdapter('candle');
        if (adapter && adapter.getQueueStats) {
          return adapter.getQueueStats();
        }
      } catch {
        // AIProviderDaemon not initialized yet - return defaults
      }
      return { queueSize: 0, activeRequests: 0, maxConcurrent: 1, load: 0.0 };
    });

    // Self-task generation now runs in Rust (ChannelModule.tick() → SelfTaskGenerator)

    // Tool result tracking (prevents infinite response loops)
    this.taskTracker = new PersonaTaskTracker();

    // Initialize logger FIRST - other subsystems need it
    this.logger = new PersonaLogger(this);

    // Wire up module-level loggers (these are singleton modules, not per-persona)
    // Use cognition.log for tool definitions and peer review (both are cognition-related)
    setToolDefinitionsLogger((message: string) => {
      this.logger.enqueueLog('cognition.log', message);
    });
    setPeerReviewLogger((message: string) => {
      this.logger.enqueueLog('cognition.log', message);
    });

    // NEUROANATOMY Phase 1: Initialize Limbic System FIRST (memory, learning, identity, emotion)
    // Limbic wraps memory/genome/learning systems - must be initialized before anything that uses getters
    this.limbic = new LimbicSystem(this as any as PersonaUserForLimbic);

    // NEUROANATOMY Phase 2: Initialize Prefrontal Cortex (cognition, evaluation, planning)
    this.prefrontal = new PrefrontalCortex(this as any as PersonaUserForPrefrontal);

    // NEUROANATOMY Phase 3: Initialize Motor Cortex (action, execution, output)
    // Note: Motor cortex creates toolExecutor, toolRegistry, and responseGenerator internally
    this.motorCortex = new MotorCortex({
      id: this.id,
      displayName: this.displayName,
      entity: this.entity,
      modelConfig: this.modelConfig,
      client,
      mediaConfig: this.mediaConfig,
      getSessionId: () => this.sessionId,
      homeDirectory: this.homeDirectory,
      logger: this.logger,
      memory: this.memory,  // For accessing trained LoRA adapters during inference
      ensureCodeWorkspace: async () => {
        this.log.debug(`🔧 ensureCodeWorkspace called, ${this._workspaces.size} existing workspaces`);
        // Reuse any existing workspace (project or sandbox) before creating a new one.
        // This allows workspaces created via explicit commands to be preserved.
        const existing = this._workspaces.get('default') ?? this._workspaces.values().next().value;
        if (existing) {
          // CRITICAL: Always re-register with Rust using persona UUID (not handle).
          // This ensures Rust CodeModule can look up the workspace by personaId.
          // Idempotent - Rust DashMap will just update the existing entry.
          // Include the main repo as a read root so personas can explore the codebase.
          const repoPath = process.cwd();  // JTAG root is the repo
          await CodeDaemon.createWorkspace(this.id, existing.dir, [repoPath]);
          // Ensure shell session exists even for pre-existing workspaces.
          // code/shell/* commands call CodeDaemon directly (bypass Workspace object),
          // so the Rust-side shell session must be eagerly created.
          await existing.ensureShell();
          return;
        }
        // Default to project mode: all personas get git worktree branches on the shared repo.
        // This enables collaboration — AIs can see each other's branches, review, merge.
        // WorkspaceStrategy auto-detects the git root from process.cwd().
        const ws = await this.ensureWorkspace({
          contextKey: 'default',
          mode: 'project',
          repoPath: process.cwd(),
        });
        await ws.ensureShell();
      },
    });

    // RUST COGNITION: Fast-path decision engine via IPC
    // Logs to: .continuum/personas/{uniqueId}/logs/rust-cognition.log
    this._rustCognition = new RustCognitionBridge(this as any as PersonaUserForRustCognition);

    // UNIFIED CONSCIOUSNESS: Cross-context awareness (no severance!)
    // Sits above limbic/prefrontal, provides global timeline and peripheral awareness
    this._consciousness = new UnifiedConsciousness(
      this.id,
      this.entity.uniqueId,  // e.g., "together" - matches folder name for timeline.json
      this.displayName,
      {
        debug: (msg) => this.log.debug(msg),
        info: (msg) => this.log.info(msg),
        warn: (msg) => this.log.warn(msg),
        error: (msg) => this.log.error(msg)
      }
    );
    // Register with GlobalAwarenessSource so RAG can access consciousness
    registerConsciousness(this.id, this._consciousness);
    // Wire Rust bridge into consciousness for timeline event corpus coherence
    if (this._rustCognition) {
      this._consciousness.setRustBridge(this._rustCognition);
      // Wire into response generator for text similarity (kills TS Jaccard duplicates)
      this.motorCortex!.responseGenerator.setRustBridge(this._rustCognition);
    }
    this.log.info(`🧠 ${this.displayName}: UnifiedConsciousness initialized (cross-context awareness enabled)`);

    // Logger for cognition.log (used by task executor and other modules)
    const cognitionLogger = (message: string, ...args: any[]) => {
      this.logger.enqueueLog('cognition.log', message);
    };
    // NOTE: DecisionAdapterChain removed - Rust cognition handles fast-path decisions

    // Task execution module (delegated for modularity, uses this.memory getter)
    // Pass provider so fine-tuning uses the correct adapter (Candle, OpenAI, Together, etc.)
    this.taskExecutor = new PersonaTaskExecutor(
      this.id,
      this.displayName,
      this.memory,
      this.personaState,
      this.modelConfig.provider,
      cognitionLogger
    );

    // Wire PersonaUser ref for genome reload + domain classifier sync after academy sessions
    this.taskExecutor.setPersonaUser({
      rustCognitionBridge: this.rustCognitionBridge,
      limbicSystem: {
        loadGenomeFromDatabase: () => this.limbic?.loadGenomeFromDatabase() ?? Promise.resolve(),
      },
    });

    // CNS scheduling inlined into PersonaAutonomousLoop (calls Rust serviceCycleFull directly)

    // Message evaluation module (pass PersonaUser reference for dependency injection)
    this.messageEvaluator = new PersonaMessageEvaluator(this);

    // Autonomous servicing loop module (pass PersonaUser reference for dependency injection)
    this.autonomousLoop = new PersonaAutonomousLoop(this, cognitionLogger);

    this.log.info(`🔧 ${this.displayName}: Initialized inbox, personaState, memory (genome + RAG), trainingAccumulator, toolExecutor, responseGenerator, messageEvaluator, autonomousLoop, and cognition system (workingMemory, selfState, planFormulator)`);

    // Initialize worker thread for this persona
    // Worker uses fast small model for gating decisions (should-respond check)
    this.worker = new PersonaWorkerThread(this.id, {
      providerType: 'candle',  // Always use Candle (native Rust) for fast gating (1b model)
      providerConfig: {
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

    // STEP 1.5.1: Initialize Rust cognition bridge (connects to continuum-core IPC)
    // This enables fast-path decisions (<1ms) for should-respond, priority, deduplication
    // Also wires the bridge to inbox for Rust-backed channel routing
    try {
      // Phase A: Rust bridge must init first — everything else depends on it
      await this._rustCognition?.initialize();
      if (this._rustCognition) {
        this.inbox.setRustBridge(this._rustCognition);
      }
      this.log.info(`🦀 ${this.displayName}: Rust cognition bridge connected (inbox routing enabled)`);

      // Phase B: These are independent of each other — run in parallel
      // - Rate limiter sync (~5ms)
      // - Adapter sync + genome sync (~20-50ms)
      // - Corpus ORM query (~100-500ms, I/O bound)
      if (this._rustCognition) {
        const parallelTasks: Promise<void>[] = [];

        // Task 1: Sync rate limiter config to Rust
        parallelTasks.push((async () => {
          const rlConfig = this.rateLimiter.getConfig();
          await this._rustCognition!.configureRateLimiter(
            rlConfig.minSecondsBetweenResponses,
            rlConfig.maxResponsesPerSession
          );
          this.log.info(`🦀 ${this.displayName}: Rate limiter synced to Rust (min=${rlConfig.minSecondsBetweenResponses}s, max=${rlConfig.maxResponsesPerSession})`);
        })());

        // Task 2: Sync genome adapters to Rust for model selection + LRU eviction
        if (this.memory?.genome) {
          parallelTasks.push((async () => {
            const adapters = this.memory!.genome.getAllAdapters().map(a => ({
              name: a.getName(),
              domain: a.getDomain(),
              ollama_model_name: a.getTrainedModelName() ?? undefined,
              is_loaded: a.isLoaded(),
              is_current: a === this.memory!.genome.getCurrentAdapter(),
              priority: a.getPriority(),
            }));
            if (adapters.length > 0) {
              await this._rustCognition!.syncAdapters(adapters as any);
              this.log.info(`🦀 ${this.displayName}: ${adapters.length} adapters synced to Rust for model selection`);
            }
            this.memory!.genome.setRustBridge(this._rustCognition!);
            await this.memory!.genome.syncToRust();
            this.log.info(`🦀 ${this.displayName}: Genome paging engine synced to Rust`);
          })());
        }

        // Task 3: Load corpus from ORM (I/O bound — overlaps with sync tasks above)
        // Then load into Rust compute engine for sub-millisecond 6-layer parallel recall
        parallelTasks.push((async () => {
          try {
            const { memories, events } = await this.loadCorpusFromORM();
            const corpusResult = await this._rustCognition!.memoryLoadCorpus(memories, events);
            this.log.info(`${this.displayName}: Rust corpus loaded — ${corpusResult.memory_count} memories (${corpusResult.embedded_memory_count} embedded), ${corpusResult.timeline_event_count} events (${corpusResult.embedded_event_count} embedded) in ${corpusResult.load_time_ms.toFixed(1)}ms`);
          } catch (error) {
            this.log.error(`${this.displayName}: Corpus load failed:`, error);
            // Non-fatal — recall will return empty results until corpus is loaded
          }
        })());

        await Promise.all(parallelTasks);
      }
    } catch (error) {
      this.log.error(`🦀 ${this.displayName}: Rust cognition init failed (messages will error):`, error);
      // Don't throw - let persona initialize, but message handling will fail loudly
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

    // STEP 1.7: Wire AI provider to genome for real LoRA adapter loading (genome vision)
    // This enables PersonaGenome.activateSkill() → CandleAdapter.applySkill() → InferenceWorker.loadAdapter()
    // Without this, adapters run in stub mode (tracking state only, no actual GPU loading)
    // NOTE: AIProviderDaemon may not be initialized yet (race condition), so use deferred wiring
    this.wireGenomeToProvider();

    // STEP 2: Subscribe to room-specific chat events (only if client available)
    if (this.client && !this.eventsSubscribed) {
      this.log.debug(`🔧 ${this.displayName}: About to subscribe to ${this.myRoomIds.size} room(s), eventsSubscribed=${this.eventsSubscribed}`);

      // Subscribe to ALL chat events once (not per-room)
      // subscribeToChatEvents() filters by this.myRoomIds internally
      this.subscribeToChatEvents(this.handleChatMessage.bind(this));
      this.subscribeToRoomUpdates(this.handleRoomUpdate.bind(this));

      // Subscribe to truncate events to reset rate limiter (using Events.subscribe)
      // Pass this.id as subscriberId to enable deduplication (prevents duplicate subscriptions)
      // MEMORY LEAK FIX: Store unsubscribe function for cleanup
      const unsubTruncate = Events.subscribe('data:chat_messages:truncated', () => {
        // Clear message deduplication cache when messages are truncated
        this.rateLimiter.clearEvaluatedMessages();
      }, undefined, this.id);
      this._eventUnsubscribes.push(unsubTruncate);

      // Subscribe to DIRECTED voice transcription events (only when arbiter selects this persona)
      const unsubVoiceTranscription = Events.subscribe('voice:transcription:directed', async (transcriptionData: {
        sessionId: UUID;
        speakerId: UUID;
        speakerName: string;
        transcript: string;
        confidence: number;
        language: string;
        timestamp: number;
        targetPersonaId: UUID;
      }) => {
        // Only process if directed at THIS persona
        if (transcriptionData.targetPersonaId === this.id) {
          this.log.info(`🎙️ ${this.displayName}: Received DIRECTED voice transcription`);
          await this.handleVoiceTranscription(transcriptionData);
        }
      }, undefined, this.id);
      this._eventUnsubscribes.push(unsubVoiceTranscription);
      this.log.info(`🎙️ ${this.displayName}: Subscribed to voice:transcription:directed events`);

      // Subscribe to TTS audio events and inject into CallServer
      // This allows AI voice responses to be heard in voice calls
      const { AIAudioInjector } = await import('../../voice/server/AIAudioInjector');
      const unsubAudioInjection = AIAudioInjector.subscribeToTTSEvents(
        this.id,
        this.displayName
      );
      this._eventUnsubscribes.push(unsubAudioInjection);
      this.log.info(`🎙️ ${this.displayName}: Subscribed to TTS audio injection events`);

      // Subscribe to shell events from Rust CodeModule (feedback loop for coding system)
      // Events: shell:{personaId}:complete, shell:{personaId}:error, shell:{personaId}:started
      // Routes shell execution results back to inbox for autonomous iteration
      const unsubShellEvents = initShellEventHandler(this.id, async (_personaId, task) => {
        await this.inbox.enqueue(task);
        this.personaState.updateInboxLoad(this.inbox.getSize());
        this.log.info(`🔧 ${this.displayName}: Shell event routed to inbox (type=${task.taskType}, priority=${task.priority.toFixed(2)})`);
      });
      this._eventUnsubscribes.push(unsubShellEvents);
      this.log.info(`🔧 ${this.displayName}: Subscribed to shell events (coding feedback loop enabled)`);

      this.eventsSubscribed = true;
      this.log.info(`✅ ${this.displayName}: Subscriptions complete, eventsSubscribed=${this.eventsSubscribed}`);

      // STARTUP CATCH-UP: Query recent messages that may have arrived during initialization
      // This prevents the race condition where messages arrive before subscriptions are active
      await this.catchUpOnRecentMessages();
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
    await this.limbic!.startMemoryConsolidation();

    // GENOME INTEGRATION: Load adapters from database into PersonaGenome
    // This bridges persisted genome (GenomeEntity) with runtime (PersonaGenome)
    await this.limbic!.loadGenomeFromDatabase();
  }

  // ════════════════════════════════════════════════════════════════════════════
  // Corpus Loading — ORM → Rust compute engine
  // ════════════════════════════════════════════════════════════════════════════

  /**
   * Load all memories + timeline events from the persona's longterm.db via ORM,
   * map from camelCase ORM entities to snake_case Rust types.
   *
   * Opens a read-only ORM handle to longterm.db, queries both collections in
   * parallel, maps entity fields, and returns typed corpus data ready for IPC.
   *
   * Data flow: longterm.db → DataOpen → DataList → field mapping → CorpusMemory[] / CorpusTimelineEvent[]
   */
  private async loadCorpusFromORM(): Promise<{ memories: CorpusMemory[], events: CorpusTimelineEvent[] }> {
    const dbPath = SystemPaths.personas.longterm(this.entity.uniqueId);

    const openResult = await DataOpen.execute({
      adapter: 'sqlite',
      config: { path: dbPath, mode: 'readwrite', wal: true, foreignKeys: true }
    });

    if (!openResult.success || !openResult.dbHandle) {
      this.log.warn(`${this.displayName}: Could not open longterm.db for corpus: ${openResult.error}`);
      return { memories: [], events: [] };
    }

    const dbHandle = openResult.dbHandle;

    // Parallel ORM queries — both are read-only against the same DB handle
    const [memResult, evtResult] = await Promise.all([
      DataList.execute({
        dbHandle,
        collection: 'memories',
        orderBy: [{ field: 'timestamp', direction: 'desc' }],
        limit: 100000,
      }),
      DataList.execute({
        dbHandle,
        collection: 'timeline_events',
        orderBy: [{ field: 'timestamp', direction: 'desc' }],
        limit: 100000,
      }),
    ]);

    // Map ORM entities (camelCase) → Rust types (snake_case)
    const memories: CorpusMemory[] = (memResult.success && memResult.items)
      ? (memResult.items as unknown as MemoryEntity[]).map(mem => this.mapMemoryToCorpus(mem))
      : [];

    const events: CorpusTimelineEvent[] = (evtResult.success && evtResult.items)
      ? (evtResult.items as unknown as Record<string, unknown>[]).map(evt => this.mapTimelineEventToCorpus(evt))
      : [];

    return { memories, events };
  }

  /**
   * Map a single MemoryEntity (camelCase ORM) to CorpusMemory (snake_case Rust).
   * Handles field renaming, default values, and embedding extraction.
   */
  private mapMemoryToCorpus(mem: MemoryEntity): CorpusMemory {
    return {
      record: {
        id: mem.id,
        persona_id: mem.personaId ?? this.entity.uniqueId,
        memory_type: mem.type,
        content: mem.content,
        context: mem.context ?? {},
        timestamp: typeof mem.timestamp === 'string'
          ? mem.timestamp
          : new Date(mem.timestamp as unknown as number).toISOString(),
        importance: mem.importance ?? 0.5,
        access_count: mem.accessCount ?? 0,
        tags: mem.tags ?? [],
        related_to: mem.relatedTo ?? [],
        source: mem.source ?? null,
        last_accessed_at: mem.lastAccessedAt ?? null,
        layer: null,           // Set by recall layers, not on input
        relevance_score: null, // Set by semantic recall, not on input
      },
      embedding: mem.embedding ?? null,
    };
  }

  /**
   * Map a single TimelineEventEntity (camelCase ORM) to CorpusTimelineEvent (snake_case Rust).
   * Uses Record<string, unknown> because DataList returns plain objects, not class instances.
   */
  private mapTimelineEventToCorpus(evt: Record<string, unknown>): CorpusTimelineEvent {
    return {
      event: {
        id: evt.id as string,
        persona_id: (evt.personaId as string) ?? this.entity.uniqueId,
        timestamp: typeof evt.timestamp === 'string'
          ? evt.timestamp
          : new Date(evt.timestamp as number).toISOString(),
        context_type: (evt.contextType as string) ?? 'room',
        context_id: (evt.contextId as string) ?? '',
        context_name: (evt.contextName as string) ?? '',
        event_type: (evt.eventType as string) ?? 'observation',
        actor_id: (evt.actorId as string) ?? '',
        actor_name: (evt.actorName as string) ?? '',
        content: (evt.content as string) ?? '',
        importance: (evt.importance as number) ?? 0.5,
        topics: (evt.topics as string[]) ?? [],
      },
      embedding: (evt.embedding as number[]) ?? null,
    };
  }

  /**
   * Override loadMyRooms to also populate room name cache for timeline events
   */
  protected override async loadMyRooms(): Promise<void> {
    await super.loadMyRooms();

    // Also populate room name cache from all rooms
    try {
      const roomsResult = await ORM.query<RoomEntity>({
        collection: COLLECTIONS.ROOMS,
        filter: {}
      });

      if (roomsResult.success && roomsResult.data) {
        for (const roomRecord of roomsResult.data) {
          const room = roomRecord.data;
          const roomId = roomRecord.id || room.id;
          this._roomNameCache.set(roomId, room.name || room.uniqueId || roomId);
        }
        this.log.debug(`📚 ${this.displayName}: Cached ${this._roomNameCache.size} room names for timeline events`);
      }
    } catch (error) {
      this.log.warn(`⚠️ ${this.displayName}: Could not cache room names: ${error}`);
    }
  }

  /**
   * Wire genome to AI provider with deferred retry if daemon not ready
   *
   * Handles the race condition where PersonaUser.onConnect() may run before
   * AIProviderDaemon is initialized. Uses retry with backoff to eventually
   * wire the genome when the daemon becomes available.
   *
   * @param retryCount - Number of retries attempted (default 0)
   * @param maxRetries - Maximum retry attempts (default 5)
   */
  private wireGenomeToProvider(retryCount: number = 0, maxRetries: number = 5): void {
    // Check if daemon is initialized
    if (!AIProviderDaemon.isInitialized()) {
      if (retryCount < maxRetries) {
        // Schedule retry with exponential backoff (2s, 4s, 8s, 16s, 32s)
        const delay = Math.pow(2, retryCount + 1) * 1000;
        this.log.debug(`🧬 ${this.displayName}: AIProviderDaemon not ready, retry ${retryCount + 1}/${maxRetries} in ${delay}ms`);
        setTimeout(() => this.wireGenomeToProvider(retryCount + 1, maxRetries), delay);
      } else {
        this.log.warn(`⚠️ ${this.displayName}: Genome wiring failed after ${maxRetries} retries - running in stub mode`);
      }
      return;
    }

    // Daemon is ready, wire the genome
    try {
      // Try to get CandleAdapter (native Rust inference with LoRA support)
      const candleAdapter = AIProviderDaemon.getAdapter('candle');
      if (candleAdapter) {
        this.memory.genome.setAIProvider(candleAdapter);
        this.log.info(`🧬 ${this.displayName}: Genome wired to CandleAdapter (LoRA composition enabled)`);
      } else {
        this.log.warn(`⚠️ ${this.displayName}: No Candle adapter available for genome`);
      }
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      this.log.warn(`⚠️ ${this.displayName}: Could not wire genome to AI provider: ${errorMsg}`);
      // Non-fatal: genome will run in stub mode
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
      this.log.warn(`⚠️ ${this.displayName}: Cannot auto-join general room - no client available`);
      return;
    }

    try {
      // Query for general room using ORM.query (server-side only)
      const queryResult = await ORM.query<RoomEntity>({
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

      // Update room with new member using ORM.update
      await ORM.update<RoomEntity>(
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
   * Catch up on messages since last processed bookmark
   * Uses roomReadState from UserStateEntity to track per-room progress
   * Ensures no messages are missed even after system restart
   */
  private async catchUpOnRecentMessages(): Promise<void> {
    try {
      const roomIds = Array.from(this.myRoomIds);
      if (roomIds.length === 0) {
        this.log.debug(`⏭️ ${this.displayName}: No rooms to catch up on`);
        return;
      }

      let totalCaughtUp = 0;

      // Process each room's bookmark independently
      for (const roomId of roomIds) {
        // Direct property access (state may be plain object from DB)
        const roomState = this.state.roomReadState?.[roomId];
        const cutoffTime = roomState?.lastReadMessageTimestamp || new Date(0).toISOString();

        const recentMessages = await ORM.query<ChatMessageEntity>({
          collection: COLLECTIONS.CHAT_MESSAGES,
          filter: {
            roomId,
            timestamp: { $gt: cutoffTime }, // Messages AFTER bookmark
            senderId: { $ne: this.id },
            senderType: { $ne: 'system' }
          },
          sort: [{ field: 'timestamp', direction: 'asc' }],
          limit: 100 // Process up to 100 per room
        });

        if (!recentMessages.success || !recentMessages.data || recentMessages.data.length === 0) {
          continue;
        }

        const messages = recentMessages.data.map(r => r.data);
        this.log.info(`🔄 ${this.displayName}: Catching up on ${messages.length} messages in room ${roomId.slice(0,8)}`);

        for (const message of messages) {
          await this.handleChatMessage(message);
        }

        totalCaughtUp += messages.length;
      }

      if (totalCaughtUp > 0) {
        this.log.info(`✅ ${this.displayName}: Catch-up complete (${totalCaughtUp} messages)`);
      }
    } catch (error) {
      this.log.warn(`⚠️ ${this.displayName}: Catch-up failed (non-fatal):`, error);
    }
  }

  /**
   * Update the room read state bookmark after processing a message
   * Persists to UserStateEntity for survival across restarts
   *
   * Called AFTER message is fully processed (response sent or decision not to respond)
   * This enables true pause/resume: shutdown mid-processing resumes on restart
   *
   * @param roomId - Room the message was in
   * @param timestamp - Message timestamp (Date or number ms)
   * @param messageId - Message ID for exact tracking
   */
  public async updateMessageBookmark(roomId: UUID, timestamp: Date | number, messageId: UUID): Promise<void> {
    try {
      const ts = typeof timestamp === 'number' ? new Date(timestamp) : timestamp;

      // Update roomReadState directly (state may be plain object from DB, not class instance)
      if (!this.state.roomReadState) {
        this.state.roomReadState = {};
      }
      this.state.roomReadState[roomId] = {
        lastReadMessageTimestamp: ts.toISOString(),
        lastReadMessageId: messageId
      };

      // Persist state change - storage.save returns result, doesn't throw
      const result = await this.storage.save(this.state);
      if (!result.success) {
        this.log.warn(`⚠️ ${this.displayName}: Bookmark save failed: ${result.error} (stateId=${this.state.id}, roomId=${roomId})`);
      } else {
        this.log.debug(`🔖 ${this.displayName}: Bookmark updated for room ${roomId.slice(0,8)} → ${ts.toISOString()}`);
      }
    } catch (error) {
      this.log.warn(`⚠️ ${this.displayName}: Failed to update bookmark: ${error instanceof Error ? error.message : String(error)}`);
      // Non-fatal - continue processing
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

    // STEP 1b: Skip system messages (tool results, etc.)
    // Tool results are stored with senderType='system' and shouldn't trigger new responses.
    // This prevents infinite loops where AI responds to its own tool results.
    if (messageEntity.senderType === 'system') {
      this.log.debug(`⏭️ ${this.displayName}: Skipping system message (tool result) from ${messageEntity.senderName}`);
      return;
    }

    // STEP 1c: Skip audio-native models for text chat (they only work in voice calls)
    // Audio-native models like Gemini Live and Qwen3-Omni communicate via direct audio I/O,
    // not through the text generation pipeline.
    const metadata = this.entity.metadata as Record<string, unknown> | undefined;
    if (metadata?.isAudioNative === true) {
      this.log.debug(`⏭️ ${this.displayName}: Skipping chat (audio-native model, voice-only)`);
      return;
    }

    // STEP 2: Deduplication - prevent evaluating same message multiple times
    // Uses TS-local Set (not Rust DashSet) because CognitionEngine.evaluated_messages
    // serves a different purpose (fast_path_decision pipeline dedup). Merging them
    // caused all messages to be skipped — channel tick marks them before PersonaUser sees them.
    if (this.rateLimiter.hasEvaluatedMessage(messageEntity.id)) {
      return; // Already evaluated this message
    }
    this.rateLimiter.markMessageEvaluated(messageEntity.id);

    // STEP 3: Skip resolved messages (moderator marked as no longer needing responses)
    if (messageEntity.metadata?.resolved) {
      this.log.debug(`⏭️ ${this.displayName}: Skipping resolved message from ${messageEntity.senderName}`);
      return;
    }

    // PHASE 3BIS: Update activity temperature (observation only, doesn't affect decisions yet)
    getChatCoordinator().onHumanMessage(messageEntity.roomId);

    // UNIFIED CONSCIOUSNESS: Record event in global timeline (cross-context awareness)
    // Fire and forget - don't block message processing
    if (this._consciousness) {
      this._consciousness.recordEvent({
        contextType: 'room',
        contextId: messageEntity.roomId,
        contextName: this.getRoomName(messageEntity.roomId), // Use human-readable room name
        eventType: 'message_received',
        actorId: messageEntity.senderId,
        actorName: messageEntity.senderName,
        content: messageEntity.content?.text || '',
        importance: 0.5, // Base importance, adjust based on mention/content later
        topics: this.extractTopics(messageEntity.content?.text || '')
      }).catch(err => this.log.warn(`Timeline record failed: ${err}`));
    }

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
      senderType: messageEntity.senderType as 'human' | 'persona' | 'agent' | 'system',
      timestamp: this.timestampToNumber(messageEntity.timestamp),
      priority
    };

    await this.inbox.enqueue(inboxMessage);

    // Update inbox load in state (for mood calculation)
    this.personaState.updateInboxLoad(this.inbox.getSize());

    // NOTE: Bookmark is NOT updated here - only after message is fully processed
    // This enables true pause/resume: shutdown mid-processing resumes on restart

    this.log.info(`📨 ${this.displayName}: Enqueued message (priority=${priority.toFixed(2)}, inbox size=${this.inbox.getSize()}, mood=${this.personaState.getState().mood})`);

    // PHASE 3: Autonomous polling loop will service inbox at adaptive cadence
    // (No immediate processing - messages wait in inbox until loop polls)
    // NOTE: Memory creation handled autonomously by Hippocampus subprocess
  }

  /**
   * Handle voice transcription from live call
   * Voice transcriptions flow through the same inbox/priority system as chat messages
   */
  private async handleVoiceTranscription(transcriptionData: {
    sessionId: UUID;
    speakerId: UUID;
    speakerName: string;
    speakerType?: 'human' | 'persona' | 'agent';  // Added: know if speaker is human or AI
    transcript: string;
    confidence: number;
    language: string;
    timestamp?: string | number;
  }): Promise<void> {
    // STEP 1: Ignore our own transcriptions
    if (transcriptionData.speakerId === this.id) {
      return;
    }

    this.log.debug(`🎤 ${this.displayName}: Received transcription from ${transcriptionData.speakerName}: "${transcriptionData.transcript.slice(0, 50)}..."`);

    // STEP 2: Deduplication - prevent evaluating same transcription multiple times
    const transcriptionKey = `${transcriptionData.speakerId}-${transcriptionData.timestamp || Date.now()}`;
    if (this.rateLimiter.hasEvaluatedMessage(transcriptionKey)) {
      return;
    }
    this.rateLimiter.markMessageEvaluated(transcriptionKey);

    // STEP 3: Calculate priority for voice transcriptions
    // Voice transcriptions from live calls should have higher priority than passive chat
    const timestamp = transcriptionData.timestamp
      ? (typeof transcriptionData.timestamp === 'number'
          ? transcriptionData.timestamp
          : new Date(transcriptionData.timestamp).getTime())
      : Date.now();

    const priority = calculateMessagePriority(
      {
        content: transcriptionData.transcript,
        timestamp,
        roomId: transcriptionData.sessionId  // Use call sessionId as "roomId" for voice
      },
      {
        displayName: this.displayName,
        id: this.id,
        recentRooms: Array.from(this.myRoomIds),
        expertise: []
      }
    );

    // Boost priority for voice (real-time conversation is more urgent than text)
    const boostedPriority = Math.min(1.0, priority + 0.2);

    // STEP 4: Enqueue to inbox as InboxMessage
    // Voice flows through the same CNS/inbox pipeline as all other modalities —
    // the inbox IS the autonomous entity's brain and decision-making system.
    const inboxMessage: InboxMessage = {
      id: generateUUID(),
      type: 'message',
      domain: 'chat',
      roomId: transcriptionData.sessionId,
      content: transcriptionData.transcript,
      senderId: transcriptionData.speakerId,
      senderName: transcriptionData.speakerName,
      senderType: transcriptionData.speakerType || 'human',
      timestamp,
      priority: boostedPriority,
      sourceModality: 'voice',
      voiceSessionId: transcriptionData.sessionId,
    };

    await this.inbox.enqueue(inboxMessage);
    this.personaState.updateInboxLoad(this.inbox.getSize());

    this.log.info(`🎙️ ${this.displayName}: Enqueued voice transcription (priority=${boostedPriority.toFixed(2)}, confidence=${transcriptionData.confidence}, inbox size=${this.inbox.getSize()})`);

    // UNIFIED CONSCIOUSNESS: Record voice event in global timeline
    if (this._consciousness) {
      this._consciousness.recordEvent({
        contextType: 'room',  // Voice call is like a room
        contextId: transcriptionData.sessionId,
        contextName: `Voice Call ${transcriptionData.sessionId.slice(0, 8)}`,
        eventType: 'message_received',  // It's a received message (via voice)
        actorId: transcriptionData.speakerId,
        actorName: transcriptionData.speakerName,
        content: transcriptionData.transcript,
        importance: 0.7,  // Higher than chat messages (real-time voice is more important)
        topics: this.extractTopics(transcriptionData.transcript)
      }).catch(err => this.log.warn(`Timeline record failed: ${err}`));
    }
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
    message: ProcessableMessage,
    senderIsHuman: boolean,
    messageText: string,
    preComputedDecision?: FastPathDecision
  ): Promise<void> {
    return await this.messageEvaluator.evaluateAndPossiblyRespondWithCognition(message, senderIsHuman, messageText, preComputedDecision);
  }

  /**
   * Evaluate message and possibly respond (called with exclusive evaluation lock)
   *
   * NOTE: Now called from evaluateAndPossiblyRespondWithCognition wrapper
   */
  private async evaluateAndPossiblyRespond(
    messageEntity: ProcessableMessage,
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
   * Convert timestamp to number (handles Date, number, string, or undefined from JSON serialization)
   * PUBLIC: Used by PersonaMessageEvaluator module
   *
   * NOTE: Rust ORM returns dates as ISO strings (e.g., "2026-02-07T18:17:56.886Z").
   * Must handle all formats to prevent type mismatch errors when passing to Rust IPC.
   */
  timestampToNumber(timestamp: Date | number | string | undefined): number {
    if (timestamp === undefined) {
      return Date.now(); // Use current time if timestamp missing
    }
    if (timestamp instanceof Date) {
      return timestamp.getTime();
    }
    if (typeof timestamp === 'string') {
      // Parse ISO string from Rust ORM (e.g., "2026-02-07T18:17:56.886Z")
      const parsed = new Date(timestamp).getTime();
      return isNaN(parsed) ? Date.now() : parsed;
    }
    return timestamp; // Already a number
  }

  /**
   * Extract topics from message content for timeline semantic linking
   *
   * For true semantics, we rely on embeddings stored in TimelineEventEntity.
   * This method returns an empty array - semantic relevance comes from
   * vector similarity search, not keyword matching.
   *
   * Future: Could use LLM to extract key concepts, but embeddings are sufficient.
   */
  private extractTopics(_text: string): string[] {
    // Return empty - semantic relevance comes from embeddings on TimelineEventEntity
    // The timeline uses vector similarity search for cross-context retrieval
    return [];
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
    originalMessage: ProcessableMessage,
    decisionContext?: Omit<LogDecisionParams, 'responseContent' | 'tokensUsed' | 'responseTime'>,
    preBuiltRagContext?: PipelineRAGContext
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

    const result = await this.responseGenerator.generateAndPostResponse(originalMessage, decisionContext, preBuiltRagContext);

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
    const timer = TimingHarness.start('persona/generate-text', 'persona');
    timer.setMeta('personaId', this.entity.uniqueId);
    timer.setMeta('displayName', this.displayName);
    timer.setMeta('context', request.context || 'unknown');
    timer.setMeta('provider', this.modelConfig.provider);
    timer.setMeta('model', this.modelConfig.model);

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
      timer.mark('build_messages');

      const genRequest: TextGenerationRequest = {
        messages,
        model: this.modelConfig.model,
        temperature: request.temperature ?? this.modelConfig.temperature ?? 0.7,
        maxTokens: request.maxTokens ?? this.modelConfig.maxTokens,
        provider: this.modelConfig.provider,
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
      timer.mark('ai_generation');
      timer.setMeta('outputLength', response.text.length);

      timer.finish();
      return response.text;
    } catch (error) {
      timer.setError(error instanceof Error ? error.message : String(error));
      timer.finish();
      this.log.error(`❌ ${this.displayName}: Text generation failed (context=${request.context || 'unknown'}): ${error}`);
      throw error;
    }
  }

  /**
   * Handle room update event
   * Updates membership tracking when this persona is added/removed from a room
   * CRITICAL: Also subscribes to chat events for newly added rooms
   */
  private async handleRoomUpdate(roomEntity: RoomEntity): Promise<void> {
    const isMember = roomEntity.members.some((m: { userId: UUID }) => m.userId === this.id);
    const wasInRoom = this.myRoomIds.has(roomEntity.id);

    // Always cache room name (even if not changing membership)
    this._roomNameCache.set(roomEntity.id, roomEntity.name || roomEntity.uniqueId || roomEntity.id);

    if (isMember && !wasInRoom) {
      // Added to room - update membership AND subscribe to chat events
      this.myRoomIds.add(roomEntity.id);
      this.subscribeToRoomChatEvents(roomEntity.id);
      this.log.info(`🚪 ${this.displayName}: Joined room ${roomEntity.name} (${roomEntity.id.slice(0,8)}) - now listening`);
    } else if (!isMember && wasInRoom) {
      // Removed from room
      this.myRoomIds.delete(roomEntity.id);
      this.log.info(`🚪 ${this.displayName}: Left room ${roomEntity.name} (${roomEntity.id.slice(0,8)})`);
    }
  }

  /**
   * Get human-readable room name for timeline events
   * Falls back to UUID if name not cached
   */
  private getRoomName(roomId: UUID): string {
    return this._roomNameCache.get(roomId) || roomId.slice(0, 8);
  }

  /**
   * Subscribe to chat events for a single room
   * Used both during initialization and when dynamically added to a room
   * MEMORY LEAK FIX: Store unsubscribe function for cleanup
   */
  private subscribeToRoomChatEvents(roomId: string): void {
    const eventName = getDataEventName(COLLECTIONS.CHAT_MESSAGES, 'created');
    const handler = this.handleChatMessage.bind(this);

    const unsub = Events.subscribe(eventName, async (messageData: ChatMessageEntity) => {
      this.log.debug(`🔔 ${this.displayName}: Event for room ${roomId.slice(0,8)}`);
      await handler(messageData);
    }, { where: { roomId } }, `${this.id}_${roomId}`);
    this._eventUnsubscribes.push(unsub);

    this.log.info(`📡 ${this.displayName}: Subscribed to chat events for room ${roomId.slice(0,8)}`);
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
    const recentMessages = await ORM.query<ChatMessageEntity>({
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
      const result = await this.client.daemons.commands.execute<DataReadParams, DataReadResult<UserEntity>>(DATA_COMMANDS.READ, {
        userId: this.client.userId,
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
   * Get persona database path — delegates to SystemPaths (single source of truth)
   */
  getPersonaDatabasePath(): string {
    return SystemPaths.personas.state(this.entity.uniqueId);
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

    const storedEntity = await ORM.store<UserEntity>(
      COLLECTIONS.USERS,
      userEntity
    );

    // STEP 2: Create UserStateEntity with persona-specific defaults
    const userState = this.getDefaultState(storedEntity.id);
    userState.preferences = getDefaultPreferencesForType('persona');

    const storedState = await ORM.store<UserStateEntity>(
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
    message: ProcessableMessage,
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
    // MEMORY LEAK FIX: Unsubscribe from all events first
    const subCount = this._eventUnsubscribes.length;
    for (const unsub of this._eventUnsubscribes) {
      try {
        unsub();
      } catch (e) {
        // Ignore errors during cleanup
      }
    }
    this._eventUnsubscribes = [];
    this.log.info(`🧹 ${this.displayName}: Cleaned up ${subCount} event subscriptions`);

    // Clear room name cache
    this._roomNameCache.clear();

    // Unregister consciousness from GlobalAwarenessSource
    if (this._consciousness) {
      unregisterConsciousness(this.id);
      this._consciousness = null;
      this.log.info(`🧠 ${this.displayName}: UnifiedConsciousness unregistered`);
    }

    // Stop soul systems (hippocampus + memory consolidation)
    await this.limbic!.shutdown();

    // Force flush all queued logs before stopping logger
    await this.logger.forceFlush();

    // Stop logger last (ensure all logs written)
    await this.logger.stop();
    this.log.info(`📝 ${this.displayName}: PersonaLogger stopped (all logs flushed)`);

    // Stop autonomous servicing loop
    await this.autonomousLoop.stopServicing();

    // Clean up all workspaces (shell sessions + worktrees)
    for (const [key, ws] of this._workspaces) {
      try {
        await ws.destroy();
        this.log.info(`${this.displayName}: Workspace destroyed (context=${key})`);
      } catch (e) {
        this.log.warn(`${this.displayName}: Workspace cleanup failed (context=${key}): ${e}`);
      }
    }
    this._workspaces.clear();

    // PHASE 6: Shutdown memory module (genome + RAG)
    await this.memory.shutdown();

    if (this.worker) {
      await this.worker.shutdown();
      this.log.info(`🧵 ${this.displayName}: Worker thread shut down`);
      this.worker = null;
    }
  }

}
