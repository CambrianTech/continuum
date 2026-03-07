/**
 * ChatRAGBuilder - Builds LLM context from chat room conversations
 *
 * Loads:
 * - Recent message history from database
 * - Image attachments from messages (for vision models)
 * - Persona identity (name, bio, system prompt)
 * - Private memories (observations, patterns, reflections)
 *
 * Used by PersonaUser to generate contextually-aware responses
 */

import { RAGBuilder } from '../shared/RAGBuilder';
import type {
  RAGContext,
  RAGBuildOptions,
  RAGDomain,
  LLMMessage,
  RAGArtifact,
  PersonaIdentity,
  PersonaMemory,
  RecipeStrategy,
} from '../shared/RAGTypes';
import type { RecipeToolDeclaration } from '../../recipes/shared/RecipeTypes';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import { ORM } from '../../../daemons/data-daemon/server/ORM';
import { RoomEntity } from '../../data/entities/RoomEntity';
import { RecipeLoader } from '../../recipes/server/RecipeLoader';
import type { StageCompleteEvent } from '../../conversation/shared/CognitionEventTypes';
import { calculateSpeedScore, getStageStatus, COGNITION_EVENTS } from '../../conversation/shared/CognitionEventTypes';
import { Events } from '../../core/shared/Events';
import { getContextWindow, getLatencyAwareTokenLimit, isSlowLocalModel, getInferenceSpeed } from '../../shared/ModelContextWindows';
import { HumanPresenceTracker } from '../../user/server/HumanPresenceTracker';

// RAGSource pattern imports
import { RAGComposer } from '../shared/RAGComposer';
import type { RAGSourceContext, RAGCompositionResult, RAGSection } from '../shared/RAGSource';
import {
  ConversationHistorySource,
  SemanticMemorySource,
  WidgetContextSource,
  PersonaIdentitySource,
  GlobalAwarenessSource,
  SocialMediaRAGSource,
  CodeToolSource,
  ProjectContextSource,
  GovernanceSource,
  ActivityContextSource,
  ToolDefinitionsSource,
  DocumentationSource,
  ToolMethodologySource,
  OpenProposalsSource,
  CodebaseSearchSource,
  MediaArtifactSource,
  LiveRoomAwarenessSource
} from '../sources';

/**
 * Chat-specific RAG builder
 * Converts chat room conversations into LLM-ready context
 */
export class ChatRAGBuilder extends RAGBuilder {
  readonly domain: RAGDomain = 'chat';
  private log: (message: string, ...args: any[]) => void;

  // RAGComposer for modular, parallelized source loading
  private composer: RAGComposer | null = null;

  // Per-operation timing for legacy phase diagnostics
  private _lastRecipeMs?: number;
  private _lastLearningMs?: number;

  // ── Static caches ────────────────────────────────────────────────
  // Room entity cache — shared across all persona RAG builds.
  // Rooms don't change during normal operation. 60s TTL is safety net only.
  private static _roomCache: Map<string, { entity: RoomEntity; cachedAt: number }> = new Map();
  private static readonly ROOM_CACHE_TTL_MS = 60_000;

  // Single-flight coalescing for room reads — prevents duplicate DB calls
  // when loadRecipeContext + loadLearningConfig hit getCachedRoom simultaneously.
  private static _roomInflight: Map<string, Promise<RoomEntity | null>> = new Map();

  /**
   * Get a room entity from cache or DB with single-flight coalescing.
   * Multiple concurrent callers for the same roomId share one DB read.
   */
  private static async getCachedRoom(roomId: UUID): Promise<RoomEntity | null> {
    const cached = ChatRAGBuilder._roomCache.get(roomId);
    if (cached && Date.now() - cached.cachedAt < ChatRAGBuilder.ROOM_CACHE_TTL_MS) {
      return cached.entity;
    }

    // Single-flight: if another call is already reading this room, piggyback on it
    const inflight = ChatRAGBuilder._roomInflight.get(roomId);
    if (inflight) return inflight;

    const promise = (async () => {
      const room = await ORM.read<RoomEntity>(RoomEntity.collection, roomId, 'default');
      if (room) {
        ChatRAGBuilder._roomCache.set(roomId, { entity: room, cachedAt: Date.now() });
      }
      return room;
    })();

    ChatRAGBuilder._roomInflight.set(roomId, promise);
    try {
      return await promise;
    } finally {
      ChatRAGBuilder._roomInflight.delete(roomId);
    }
  }

  constructor(logger?: (message: string, ...args: any[]) => void) {
    super();
    // Default to console.log if no logger provided (for tests)
    this.log = logger || console.log.bind(console);
  }

  /**
   * Initialize the RAGComposer with chat-relevant sources
   * Sources are loaded in parallel for better performance
   */
  private getComposer(): RAGComposer {
    if (!this.composer) {
      this.composer = new RAGComposer();
      this.composer.registerAll([
        new PersonaIdentitySource(),     // Priority 95: Who the AI is
        new GlobalAwarenessSource(),     // Priority 85: Cross-context awareness (no severance!)
        new ConversationHistorySource(), // Priority 80: Chat messages (uses queryWithJoin!)
        new WidgetContextSource(),       // Priority 75: UI state from Positron
        new SemanticMemorySource(),      // Priority 60: Long-term memories
        new ProjectContextSource(),      // Priority 70: Project workspace context (git, team, build)
        new CodebaseSearchSource(),      // Priority 55: Semantic code search from indexed codebase
        new SocialMediaRAGSource(),      // Priority 55: Social media HUD (engagement duty)
        new CodeToolSource(),            // Priority 50: Coding workflow guidance
        new ToolMethodologySource(),     // Priority 48: Non-code tool workflow guidance
        new ToolDefinitionsSource(),     // Priority 45: Tool definitions (native/XML, budget-aware)
        new ActivityContextSource(),     // Priority 40: Recipe/activity context
        new DocumentationSource(),       // Priority 35: System documentation awareness
        new OpenProposalsSource(),        // Priority 25: Open voting proposals (actionable)
        new GovernanceSource(),          // Priority 20: Democratic participation guidance
        new MediaArtifactSource(),       // Priority 65: Media artifacts (images, files) with vision preprocessing
        new LiveRoomAwarenessSource()    // Priority 30: Live call participant awareness
      ]);
      this.log('🔧 ChatRAGBuilder: Initialized RAGComposer with 16 sources');
    }
    return this.composer;
  }

  /**
   * Extract loaded data from RAGCompositionResult sections.
   *
   * GENERIC: Collects all systemPromptSection values into a Map keyed by sourceName.
   * Adding a new RAGSource requires ZERO changes here — the source's systemPromptSection
   * is automatically collected and injected into the persona's system prompt.
   *
   * Only structured data (identity, messages, memories, tool metadata) needs explicit extraction
   * because these have special shapes that can't be expressed as a system prompt string.
   */
  private extractFromComposition(result: RAGCompositionResult): {
    identity: PersonaIdentity | null;
    conversationHistory: LLMMessage[];
    memories: PersonaMemory[];
    artifacts: RAGArtifact[];
    systemPromptSections: Map<string, string>;
    toolDefinitionsMetadata: Record<string, unknown> | null;
  } {
    let identity: PersonaIdentity | null = null;
    let conversationHistory: LLMMessage[] = [];
    let memories: PersonaMemory[] = [];
    let artifacts: RAGArtifact[] = [];
    const systemPromptSections = new Map<string, string>();
    let toolDefinitionsMetadata: Record<string, unknown> | null = null;

    for (const section of result.sections) {
      // Structured data — extracted by type, not by source name
      if (section.identity) identity = section.identity;
      if (section.messages && section.messages.length > 0) conversationHistory = section.messages;
      if (section.memories && section.memories.length > 0) memories = section.memories;
      if (section.artifacts && section.artifacts.length > 0) artifacts = section.artifacts;

      // Generic: every source's systemPromptSection collected by name
      if (section.systemPromptSection) {
        systemPromptSections.set(section.sourceName, section.systemPromptSection);
      }

      // Tool definitions need metadata extraction (native tool specs for non-XML providers)
      if (section.sourceName === 'tool-definitions' && section.metadata) {
        toolDefinitionsMetadata = section.metadata;
      }
    }

    return { identity, conversationHistory, memories, artifacts, systemPromptSections, toolDefinitionsMetadata };
  }

  /**
   * Build RAG context from a chat room
   *
   * @param contextId - Room ID
   * @param personaId - The persona requesting context
   * @param options - Configuration (message limit, memory limit, etc.)
   */
  async buildContext(
    contextId: UUID,  // Room ID
    personaId: UUID,
    options: RAGBuildOptions
  ): Promise<RAGContext> {
    const startTime = Date.now();

    // Calculate safe message count based on model context window (Bug #5 fix)
    const maxMessages = this.calculateSafeMessageCount(options);
    const maxMemories = options?.maxMemories ?? 10;
    const includeArtifacts = options?.includeArtifacts ?? true;
    const includeMemories = options?.includeMemories ?? true;

    let identity: PersonaIdentity;
    let conversationHistory: LLMMessage[];
    let artifacts: RAGArtifact[];
    let privateMemories: PersonaMemory[];
    let recipeStrategy: RecipeStrategy | undefined;
    let recipeTools: RecipeToolDeclaration[] | undefined;
    let learningConfig: { learningMode?: 'fine-tuning' | 'inference-only'; genomeId?: UUID; participantRole?: string } | undefined;
    let systemPromptSections = new Map<string, string>();
    let toolDefinitionsMetadata: Record<string, unknown> | null = null;
    let composeMs: number | undefined;
    let legacyMs: number | undefined;
    // Token budget from model's context window — 75% for input.
    const contextWindow = getContextWindow(options.modelId, options.provider);
    let totalBudget = Math.floor(contextWindow * 0.75);

    {
      const composer = this.getComposer();

      if (isSlowLocalModel(options.modelId, options.provider)) {
        this.log(`📊 ChatRAGBuilder: Slow model budget=${totalBudget} (contextWindow=${contextWindow}, 75%) for ${options.provider}/${options.modelId}`);
      }

      // Load recipe + learning config in parallel (both needed before compose)
      const legacyStart = performance.now();
      const recipePromise = this.loadRecipeContext(contextId, options.recipeId);
      const learningPromise = this.loadLearningConfig(contextId, personaId);

      const [extractedRecipeContext, extractedLearningConfig] = await Promise.all([
        recipePromise.then(r => { this._lastRecipeMs = performance.now() - legacyStart; return r; }),
        learningPromise.then(r => { this._lastLearningMs = performance.now() - legacyStart; return r; })
      ]);
      recipeStrategy = extractedRecipeContext?.strategy;
      recipeTools = extractedRecipeContext?.tools;
      learningConfig = extractedLearningConfig;

      // Build source context — pass recipe's source activation list if declared
      const sourceContext: RAGSourceContext = {
        personaId,
        roomId: contextId,
        sessionId: options?.sessionId,
        options: {
          ...options,
          maxMessages,
          maxMemories,
          includeMemories,
          currentMessage: options?.currentMessage
        },
        totalBudget,
        provider: options.provider,
        toolCapability: options?.toolCapability,
        activeSources: extractedRecipeContext?.ragTemplateSources,
      };

      // Load core sources via composer (parallel, filtered by recipe's activeSources)
      // This populates ConversationHistorySource cache — artifact extraction depends on it
      const composeStart = performance.now();
      const composition = await composer.compose(sourceContext);
      composeMs = performance.now() - composeStart;
      const extracted = this.extractFromComposition(composition);
      legacyMs = performance.now() - legacyStart;

      // Artifacts now come from MediaArtifactSource via composition (parallel with other sources).
      // No separate extraction step needed — ConversationHistorySource cache is read by
      // MediaArtifactSource during the same compose phase. Vision preprocessing happens inside the source.
      artifacts = extracted.artifacts;

      // Use composed data, with fallbacks for missing pieces
      identity = extracted.identity ?? {
        name: 'AI Assistant',
        systemPrompt: 'You are a helpful AI assistant participating in a group chat.'
      };
      conversationHistory = extracted.conversationHistory;
      privateMemories = extracted.memories;
      systemPromptSections = extracted.systemPromptSections;
      toolDefinitionsMetadata = extracted.toolDefinitionsMetadata;

      this.log(`🔧 ChatRAGBuilder: Composed from ${composition.sections.length} sources in ${composition.totalLoadTimeMs.toFixed(1)}ms (compose=${composeMs.toFixed(1)}ms, legacy=${legacyMs.toFixed(1)}ms [recipe=${this._lastRecipeMs?.toFixed(1)}ms, learning=${this._lastLearningMs?.toFixed(1)}ms], artifacts=${artifacts.length})`);
    }

    // Artifacts already preprocessed by MediaArtifactSource during compose
    const processedArtifacts = artifacts;
    const preprocessMs = 0;

    // SMALL-CONTEXT GUARD: For models with tight context windows (Candle 2048 tokens),
    // skip all non-essential injections. The system prompt + conversation must fit.
    // totalBudget is 75% of contextWindow: budget 3000 ≈ 4K context window.
    // Any model under ~4K context should skip injections — there's no room.
    const isSmallContext = totalBudget < 3000;

    // 2.4. Inject RAG source context into system prompt — GENERIC LOOP
    // Each RAGSource provides a systemPromptSection. We inject them all without
    // knowing source names. Adding a new source requires ZERO changes here.
    const finalIdentity = { ...identity };

    // 2.4.1. Inject human presence awareness (which room each user is viewing)
    // This is NOT a RAG source — it's lightweight synchronous state, always injected.
    const allPresence = HumanPresenceTracker.allPresence;
    if (allPresence.length > 0) {
      const lines = allPresence.map(p => {
        const viewingThis = p.roomId === contextId;
        return `- ${p.displayName} is viewing: ${p.roomName}${viewingThis ? ' (this room — they can see your response in real-time)' : ''}`;
      });
      finalIdentity.systemPrompt = finalIdentity.systemPrompt +
        `\n\n## HUMAN PRESENCE\n${lines.join('\n')}`;
    }

    // 2.4.2. Inject all RAG source systemPromptSections generically
    //
    // Sources with wrapper instructions — the section content gets wrapped with
    // additional context instructions. Eventually these wrappers should move INTO
    // the sources themselves, making this map empty.
    const SOURCE_WRAPPERS: Record<string, (section: string) => string> = {
      'widget-context': (s) =>
        `\n\n## CURRENT USER CONTEXT (What they're viewing)\n${s}\n\nUse this context to provide more relevant assistance. If they're configuring AI providers, you can proactively help with that. If they're viewing settings, anticipate configuration questions.`,
      'global-awareness': (s) =>
        `\n\n${s}\n\nIMPORTANT: You DO have access to information from other channels/rooms. Use the "Relevant Knowledge From Other Contexts" section above when answering questions. This information is from your own experiences in other conversations.`,
    };

    // Sources that MUST be injected even for small-context models.
    // Most sources are skipped in small-context mode to fit tight token budgets.
    // Codebase search is critical — if someone asks about code, they need the answer.
    const ALWAYS_INJECT = new Set(['codebase-search']);

    // Tool definitions are injected separately (native specs vs XML have different paths)
    const SKIP_GENERIC = new Set(['tool-definitions']);

    let injectedCount = 0;
    for (const [sourceName, section] of systemPromptSections) {
      if (SKIP_GENERIC.has(sourceName)) continue;
      if (isSmallContext && !ALWAYS_INJECT.has(sourceName)) continue;

      const wrapper = SOURCE_WRAPPERS[sourceName];
      finalIdentity.systemPrompt += wrapper ? wrapper(section) : `\n\n${section}`;
      injectedCount++;
      this.log(`🔧 ChatRAGBuilder: Injected ${sourceName} into system prompt`);
    }

    // 2.4.3. Inject XML tool definitions for text-based providers (budget-aware via ToolDefinitionsSource)
    const toolDefinitionsPrompt = systemPromptSections.get('tool-definitions');
    if (!isSmallContext && toolDefinitionsPrompt) {
      finalIdentity.systemPrompt += toolDefinitionsPrompt;
      injectedCount++;
      this.log(`🔧 ChatRAGBuilder: Injected tool definitions into system prompt (XML format)`);
    }

    if (isSmallContext) {
      this.log(`📦 ChatRAGBuilder: Small-context mode (budget=${totalBudget}) — injected ${injectedCount}/${systemPromptSections.size} sources for ${options.modelId}`);
    }

    // NOTE: Canvas context is now handled via the "inbox content" pattern
    // When strokes are added, they emit system messages to the canvas room
    // AIs see these in their conversation history naturally, no system prompt injection needed

    // 2.5. Append current message if provided (for messages not yet persisted)
    // Check for duplicates by comparing content + name of most recent message
    // NOTE: conversationHistory is now const from Promise.all, need to handle mutability
    const finalConversationHistory = [...conversationHistory];
    if (options?.currentMessage) {
      const lastMessage = finalConversationHistory[finalConversationHistory.length - 1];
      const isDuplicate = lastMessage &&
        lastMessage.content === options.currentMessage.content &&
        lastMessage.name === options.currentMessage.name;

      if (!isDuplicate) {
        finalConversationHistory.push(options.currentMessage);
      } else {
        this.log(`⚠️ ChatRAGBuilder: Skipping duplicate currentMessage (already in history)`);
      }
    }

    // Bug #5 fix: Calculate adjusted maxTokens based on actual input size (dimension 2)
    const budgetCalculation = this.calculateAdjustedMaxTokens(finalConversationHistory, options);

    this.log(`🔍 [ChatRAGBuilder] Budget calculation for model ${options.modelId || 'unknown'}:`, {
      inputTokenCount: budgetCalculation.inputTokenCount,
      adjustedMaxTokens: budgetCalculation.adjustedMaxTokens,
      requestedMaxTokens: options?.maxTokens,
      conversationHistoryLength: finalConversationHistory.length
    });

    const ragContext: RAGContext = {
      domain: 'chat',
      contextId,
      personaId,
      identity: finalIdentity,
      recipeStrategy,
      recipeTools,
      conversationHistory: finalConversationHistory,
      artifacts: processedArtifacts,
      privateMemories,
      learningMode: learningConfig?.learningMode,
      genomeId: learningConfig?.genomeId,
      participantRole: learningConfig?.participantRole,
      metadata: {
        messageCount: finalConversationHistory.length,
        artifactCount: processedArtifacts.length,
        memoryCount: privateMemories.length,
        builtAt: new Date(),
        recipeId: recipeStrategy?.conversationPattern,
        recipeName: recipeStrategy ? `${recipeStrategy.conversationPattern} conversation` : undefined,

        // Bug #5 fix: Two-dimensional budget (message count + maxTokens adjustment)
        adjustedMaxTokens: budgetCalculation.adjustedMaxTokens,
        inputTokenCount: budgetCalculation.inputTokenCount,

        // RAG source presence flags (computed from generic map)
        hasWidgetContext: systemPromptSections.has('widget-context'),
        hasGlobalAwareness: systemPromptSections.has('global-awareness'),
        hasSocialAwareness: systemPromptSections.has('social-media'),
        hasProjectContext: systemPromptSections.has('project-context'),

        // Tool definitions (budget-aware via ToolDefinitionsSource)
        toolDefinitions: toolDefinitionsMetadata ? {
          nativeToolSpecs: (toolDefinitionsMetadata as any).nativeToolSpecs,
          toolChoice: (toolDefinitionsMetadata as any).toolChoice,
          toolCount: (toolDefinitionsMetadata as any).toolCount,
        } : undefined,
      }
    };

    // Log per-phase timing breakdown for performance analysis
    const durationMs = Date.now() - startTime;
    this.log(`[TIMING] ChatRAGBuilder.buildContext: total=${durationMs}ms (compose=${composeMs!.toFixed(1)}ms, legacy=${legacyMs!.toFixed(1)}ms, msgs=${conversationHistory.length}, mems=${privateMemories.length}, arts=${processedArtifacts.length})`);

    // Emit cognition event for rag-build stage (FIRE-AND-FORGET: don't block on event emission)
    const totalTokens = finalConversationHistory.reduce((sum, msg) => sum + (msg.content?.length ?? 0), 0);
    const maxTokens = 128000;  // Typical context window

    // Fire-and-forget: don't await event emission, it's non-critical telemetry
    Events.emit<StageCompleteEvent>(
      DataDaemon.jtagContext!,
      COGNITION_EVENTS.STAGE_COMPLETE,
      {
        messageId: (options as any)?.messageId ?? contextId,  // Use messageId if available, fallback to contextId
        personaId,
        contextId,
        stage: 'rag-build',
        metrics: {
          stage: 'rag-build',
          durationMs,
          resourceUsed: totalTokens,
          maxResource: maxTokens,
          percentCapacity: (totalTokens / maxTokens) * 100,
          percentSpeed: calculateSpeedScore(durationMs, 'rag-build'),
          status: getStageStatus(durationMs, 'rag-build'),
          metadata: {
            messageCount: conversationHistory.length,
            artifactCount: processedArtifacts.length,
            memoryCount: privateMemories.length
          }
        },
        timestamp: Date.now()
      }
    );

    return ragContext;
  }

  getDescription(): string {
    return 'Chat room conversation builder with image support';
  }



  /**
   * Load recipe context (strategy + tools) from room's recipeId
   */
  private async loadRecipeContext(roomId: UUID, overrideRecipeId?: string): Promise<{ strategy?: RecipeStrategy; tools?: RecipeToolDeclaration[]; ragTemplateSources?: string[] } | undefined> {
    try {
      // Use override recipeId (from queue item) or fall back to room's recipe
      let recipeId = overrideRecipeId;

      if (!recipeId) {
        const room = await ChatRAGBuilder.getCachedRoom(roomId);
        if (!room) {
          this.log(`⚠️ ChatRAGBuilder: Could not load room ${roomId}, no recipe context`);
          return undefined;
        }
        recipeId = room.recipeId;
      }

      if (!recipeId) {
        this.log(`ℹ️ ChatRAGBuilder: Room ${roomId.slice(0, 8)} has no recipeId, using default behavior`);
        return undefined;
      }

      // 2. Load recipe definition from JSON file
      const recipeLoader = RecipeLoader.getInstance();
      const recipe = await recipeLoader.loadRecipe(recipeId);

      if (!recipe) {
        this.log(`⚠️ ChatRAGBuilder: Could not load recipe ${recipeId}`);
        return undefined;
      }

      const ragTemplateSources = recipe.ragTemplate?.sources;
      this.log(`✅ ChatRAGBuilder: Loaded recipe context "${recipe.displayName}" (${recipeId}) — strategy=${!!recipe.strategy}, tools=${recipe.tools?.length ?? 0}, ragSources=${ragTemplateSources?.length ?? 'all'}`);
      return {
        strategy: recipe.strategy,
        tools: recipe.tools,
        ragTemplateSources,
      };
    } catch (error) {
      this.log(`❌ ChatRAGBuilder: Error loading recipe context:`, error);
      return undefined;
    }
  }

  /**
   * Load learning configuration for persona from room membership
   * Phase 2: Per-participant learning mode
   */
  private async loadLearningConfig(
    roomId: UUID,
    personaId: UUID
  ): Promise<{ learningMode?: 'fine-tuning' | 'inference-only'; genomeId?: UUID; participantRole?: string } | undefined> {
    try {
      // 1. Load room entity (from cache — shared with loadRoomName, loadRoomMembers, etc.)
      const room = await ChatRAGBuilder.getCachedRoom(roomId);
      if (!room) {
        this.log(`⚠️ ChatRAGBuilder: Could not load room ${roomId} for learning config`);
        return undefined;
      }

      // 2. Find this persona's membership
      const member = room.members.find(m => m.userId === personaId);
      if (!member) {
        this.log(`ℹ️ ChatRAGBuilder: Persona ${personaId.slice(0, 8)} not a member of room ${roomId.slice(0, 8)}`);
        return undefined;
      }

      // 3. Return learning config if present (all fields optional)
      const config = {
        learningMode: member.learningMode,
        genomeId: member.genomeId,
        participantRole: member.participantRole
      };

      // Log learning mode status for debugging
      if (config.learningMode) {
        this.log(`🧠 ChatRAGBuilder: Persona ${personaId.slice(0, 8)} learning mode: ${config.learningMode}` +
                    `${config.participantRole ? ` (${config.participantRole})` : ''}` +
                    `${config.genomeId ? ` genome=${config.genomeId.slice(0, 8)}` : ''}`);
      }

      return config;
    } catch (error) {
      this.log(`❌ ChatRAGBuilder: Error loading learning config:`, error);
      return undefined;
    }
  }

  /**
   * Calculate message fetch limit for artifact scanning and legacy paths.
   *
   * ConversationHistorySource enforces the real token budget by accumulating
   * actual token counts — this method only provides a generous fetch limit.
   * For slow local models, latency-aware budgeting reduces the fetch count
   * to avoid loading messages that would definitely exceed the latency cap.
   */
  private calculateSafeMessageCount(options: RAGBuildOptions): number {
    if (options?.maxMessages !== undefined) {
      return options.maxMessages;
    }

    const modelId = options.modelId;
    const contextWindow = getContextWindow(modelId, options.provider);
    const isSlowModel = isSlowLocalModel(modelId, options.provider);

    if (isSlowModel) {
      // For slow local models, use latency-aware constraint to avoid fetching
      // way more messages than could ever be processed within timeout.
      const latencyInputLimit = getLatencyAwareTokenLimit(modelId, undefined, options.provider);
      const inferenceSpeed = getInferenceSpeed(modelId, options.provider);
      const systemPromptTokens = options.systemPromptTokens ?? 500;
      const latencyBudget = latencyInputLimit - systemPromptTokens;
      // Rough estimate for fetch limit only — real enforcement is in the source
      const fetchLimit = Math.max(5, Math.floor(latencyBudget / 200));

      this.log(`📊 ChatRAGBuilder: Slow model fetch limit for ${modelId}: ${fetchLimit} (${inferenceSpeed} TPS, latency budget=${latencyBudget})`);
      return fetchLimit;
    }

    // For fast models, generous fetch limit — token budget enforcement
    // happens in ConversationHistorySource via actual token accumulation.
    // context window / 200 chars avg msg / 3 chars per token ≈ generous upper bound
    const generousFetchLimit = Math.max(50, Math.floor(contextWindow / 600));

    this.log(`📊 ChatRAGBuilder: Fetch limit for ${modelId}: ${generousFetchLimit} (contextWindow=${contextWindow})`);
    return generousFetchLimit;
  }

  /**
   * Calculate adjusted maxTokens based on actual input size (Bug #5 fix - dimension 2)
   *
   * The AI team identified this issue: we trim messages to fit, but still request
   * 3000 completion tokens, causing: inputTokens + 3000 > contextWindow
   *
   * Solution: adjustedMaxTokens = Math.min(maxTokens, contextWindow - inputTokens - safetyMargin)
   *
   * @param conversationHistory - The actual messages being sent (after trimming)
   * @param options - Build options with modelId and maxTokens
   * @returns {adjustedMaxTokens, inputTokenCount} for use in LLM call
   */
  private calculateAdjustedMaxTokens(
    conversationHistory: LLMMessage[],
    options: RAGBuildOptions
  ): { adjustedMaxTokens: number; inputTokenCount: number } {
    const requestedMaxTokens = options.maxTokens;
    const modelId = options.modelId;
    const systemPromptTokens = options.systemPromptTokens ?? 500;
    const safetyMargin = 100;  // Extra buffer for formatting/metadata
    const contextWindow = getContextWindow(modelId, options.provider);

    // Estimate input tokens from actual message content.
    // Llama tokenizer averages ~3.0 chars/token (measured: 8091 chars → 2701 tokens).
    // Using actual content is far more accurate than a flat per-message average.
    const CHARS_PER_TOKEN = 3;
    const estimatedMessageTokens = conversationHistory.reduce(
      (sum, msg) => sum + Math.ceil(msg.content.length / CHARS_PER_TOKEN), 0
    );
    const inputTokenCount = estimatedMessageTokens + systemPromptTokens;

    // Calculate available tokens for completion
    const availableForCompletion = contextWindow - inputTokenCount - safetyMargin;

    // Adjust maxTokens to fit within available space.
    // Never exceed the config value — it exists for a reason (e.g. Candle models at 200).
    // If budget is blown (availableForCompletion <= 0), return 0 — caller must handle.
    // Previous bug: Math.max(50, ...) forced 50 tokens even when budget was -752,
    // causing Rust backend to reject with "exceeds context length".
    const adjustedMaxTokens = Math.max(
      0,
      Math.min(requestedMaxTokens, availableForCompletion)
    );

    this.log(`📊 ChatRAGBuilder: Two-dimensional budget for ${modelId}:
  Context Window: ${contextWindow} tokens
  Input Tokens (estimated): ${inputTokenCount} (${estimatedMessageTokens} from content @ ${CHARS_PER_TOKEN} chars/tok + ${systemPromptTokens} system)
  Available for Completion: ${availableForCompletion}
  Requested maxTokens: ${requestedMaxTokens}
  Adjusted maxTokens: ${adjustedMaxTokens}${availableForCompletion <= 0 ? ' ❌ BUDGET BLOWN' : adjustedMaxTokens < requestedMaxTokens ? ' ⚠️ REDUCED' : ' ✓'}`);

    return { adjustedMaxTokens, inputTokenCount };
  }
}
