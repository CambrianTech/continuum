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
  RecipeStrategy
} from '../shared/RAGTypes';
import type { RecipeToolDeclaration } from '../../recipes/shared/RecipeTypes';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import { ChatMessageEntity } from '../../data/entities/ChatMessageEntity';
import { UserEntity } from '../../data/entities/UserEntity';
import { RoomEntity } from '../../data/entities/RoomEntity';
import { RecipeLoader } from '../../recipes/server/RecipeLoader';
import type { StageCompleteEvent } from '../../conversation/shared/CognitionEventTypes';
import { calculateSpeedScore, getStageStatus, COGNITION_EVENTS } from '../../conversation/shared/CognitionEventTypes';
import { Events } from '../../core/shared/Events';
import { getContextWindow, getLatencyAwareTokenLimit, isSlowLocalModel, getInferenceSpeed } from '../../shared/ModelContextWindows';
import { WidgetContextService } from '../services/WidgetContextService';
import { VisionDescriptionService } from '../../vision/VisionDescriptionService';

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
  CodeToolSource
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
  private useModularSources = true;  // Feature flag for gradual migration

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
        new SocialMediaRAGSource(),      // Priority 55: Social media HUD (engagement duty)
        new CodeToolSource()             // Priority 50: Coding workflow guidance
      ]);
      this.log('🔧 ChatRAGBuilder: Initialized RAGComposer with 7 sources');
    }
    return this.composer;
  }

  /**
   * Extract loaded data from RAGCompositionResult sections
   * Maps RAGSection fields to the format expected by buildContext
   */
  private extractFromComposition(result: RAGCompositionResult): {
    identity: PersonaIdentity | null;
    conversationHistory: LLMMessage[];
    memories: PersonaMemory[];
    widgetContext: string | null;
    globalAwareness: string | null;
    socialAwareness: string | null;
    codeToolGuidance: string | null;
  } {
    let identity: PersonaIdentity | null = null;
    let conversationHistory: LLMMessage[] = [];
    let memories: PersonaMemory[] = [];
    let widgetContext: string | null = null;
    let globalAwareness: string | null = null;
    let socialAwareness: string | null = null;
    let codeToolGuidance: string | null = null;

    for (const section of result.sections) {
      if (section.identity) {
        identity = section.identity;
      }
      if (section.messages && section.messages.length > 0) {
        conversationHistory = section.messages;
      }
      if (section.memories && section.memories.length > 0) {
        memories = section.memories;
      }
      if (section.systemPromptSection && section.sourceName === 'widget-context') {
        // Extract the raw context from the formatted section
        widgetContext = section.systemPromptSection;
      }
      if (section.systemPromptSection && section.sourceName === 'global-awareness') {
        // Extract cross-context awareness (no severance!)
        globalAwareness = section.systemPromptSection;
      }
      if (section.systemPromptSection && section.sourceName === 'social-media') {
        // Social media HUD — engagement awareness and duty
        socialAwareness = section.systemPromptSection;
      }
      if (section.systemPromptSection && section.sourceName === 'code-tools') {
        // Coding workflow guidance — code/* tool awareness
        codeToolGuidance = section.systemPromptSection;
      }
    }

    return { identity, conversationHistory, memories, widgetContext, globalAwareness, socialAwareness, codeToolGuidance };
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
    options?: RAGBuildOptions
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
    let widgetContext: string | null;
    let globalAwareness: string | null;
    let socialAwareness: string | null;
    let codeToolGuidance: string | null;

    if (this.useModularSources) {
      // NEW PATH: Use RAGComposer for modular, parallelized source loading
      // Benefits: queryWithJoin for messages (4.5x faster), testable sources, budget allocation
      const composer = this.getComposer();

      // Calculate token budget based on model capabilities
      // Local models get MINIMAL context - they can query for more via tools
      let totalBudget = 8000;  // Default for capable models
      if (options?.modelId && isSlowLocalModel(options.modelId)) {
        const latencyLimit = getLatencyAwareTokenLimit(options.modelId);
        // Local models: minimal system prompt (~500 tokens), rest for messages
        totalBudget = Math.min(totalBudget, latencyLimit - 500);
        this.log(`📊 ChatRAGBuilder: Local model budget=${totalBudget} for ${options.modelId}`);
      }

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
        totalBudget
      };

      // Load core sources via composer (parallel)
      const composition = await composer.compose(sourceContext);
      const extracted = this.extractFromComposition(composition);

      // Use composed data, with fallbacks for missing pieces
      identity = extracted.identity ?? {
        name: 'AI Assistant',
        systemPrompt: 'You are a helpful AI assistant participating in a group chat.'
      };
      conversationHistory = extracted.conversationHistory;
      privateMemories = extracted.memories;
      widgetContext = extracted.widgetContext;
      globalAwareness = extracted.globalAwareness;
      socialAwareness = extracted.socialAwareness;
      codeToolGuidance = extracted.codeToolGuidance;

      // Still load these via legacy methods (not yet extracted to sources)
      const [extractedArtifacts, extractedRecipeContext, extractedLearningConfig] = await Promise.all([
        includeArtifacts ? this.extractArtifacts(contextId, maxMessages) : Promise.resolve([]),
        this.loadRecipeContext(contextId),
        this.loadLearningConfig(contextId, personaId)
      ]);
      artifacts = extractedArtifacts;
      recipeStrategy = extractedRecipeContext?.strategy;
      recipeTools = extractedRecipeContext?.tools;
      learningConfig = extractedLearningConfig;

      this.log(`🔧 ChatRAGBuilder: Composed from ${composition.sections.length} sources in ${composition.totalLoadTimeMs.toFixed(1)}ms`);

    } else {
      // LEGACY PATH: Direct parallel loading (fallback)
      // PARALLELIZED: All these queries are independent, run them concurrently
      // This reduces RAG context build time from ~240ms (sequential) to ~40ms (parallel)
      const [
        loadedIdentity,
        loadedConversationHistory,
        loadedArtifacts,
        loadedPrivateMemories,
        loadedRecipeContext,
        loadedLearningConfig,
        loadedWidgetContext
      ] = await Promise.all([
        // 1. Load persona identity (with room context for system prompt)
        this.loadPersonaIdentity(personaId, contextId, options),

        // 2. Load recent conversation history from database
        // NOTE: Canvas activity is now visible as chat messages (inbox content pattern)
        // Strokes emit system messages to the canvas room, so AIs see them naturally here
        this.loadConversationHistory(contextId, personaId, maxMessages),

        // 3. Extract image attachments from messages (for vision models)
        includeArtifacts ? this.extractArtifacts(contextId, maxMessages) : Promise.resolve([]),

        // 4. Load private memories (semantic recall uses currentMessage for context-aware retrieval)
        includeMemories ? this.loadPrivateMemories(
          personaId,
          contextId,
          maxMemories,
          options?.currentMessage?.content  // ← Semantic query: use current message for relevant memory recall
        ) : Promise.resolve([]),

        // 5. Load room's recipe context (strategy + tool highlights)
        this.loadRecipeContext(contextId),

        // 6. Load learning configuration (Phase 2: Per-participant learning mode)
        this.loadLearningConfig(contextId, personaId),

        // 7. Load widget context for AI awareness (Positron Layer 1)
        this.loadWidgetContext(options)
      ]);

      identity = loadedIdentity;
      conversationHistory = loadedConversationHistory;
      artifacts = loadedArtifacts;
      privateMemories = loadedPrivateMemories;
      recipeStrategy = loadedRecipeContext?.strategy;
      recipeTools = loadedRecipeContext?.tools;
      learningConfig = loadedLearningConfig;
      widgetContext = loadedWidgetContext;
      globalAwareness = null;  // Legacy path doesn't use GlobalAwarenessSource
      socialAwareness = null;  // Legacy path doesn't use SocialMediaRAGSource
      codeToolGuidance = null; // Legacy path doesn't use CodeToolSource
    }

    // 2.3.5 Preprocess artifacts for non-vision models ("So the blind can see")
    // If target model can't see images, generate text descriptions
    const processedArtifacts = await this.preprocessArtifactsForModel(artifacts, options);

    // 2.4. Inject widget context into system prompt if available
    // This enables AI to be aware of what the user is currently viewing
    const finalIdentity = { ...identity };
    if (widgetContext) {
      finalIdentity.systemPrompt = identity.systemPrompt +
        `\n\n## CURRENT USER CONTEXT (What they're viewing)\n${widgetContext}\n\nUse this context to provide more relevant assistance. If they're configuring AI providers, you can proactively help with that. If they're viewing settings, anticipate configuration questions.`;
      this.log('🧠 ChatRAGBuilder: Injected widget context into system prompt');
    }

    // 2.4.5. Inject cross-context awareness into system prompt (NO SEVERANCE!)
    // This gives AIs unified knowledge that flows between rooms/contexts
    if (globalAwareness) {
      finalIdentity.systemPrompt = finalIdentity.systemPrompt +
        `\n\n${globalAwareness}\n\nIMPORTANT: You DO have access to information from other channels/rooms. Use the "Relevant Knowledge From Other Contexts" section above when answering questions. This information is from your own experiences in other conversations.`;
      this.log('🌐 ChatRAGBuilder: Injected cross-context awareness into system prompt');
    }

    // 2.4.6. Inject social media HUD into system prompt (engagement awareness)
    // This gives AIs awareness of their social media presence and engagement duty
    if (socialAwareness) {
      finalIdentity.systemPrompt = finalIdentity.systemPrompt +
        `\n\n${socialAwareness}`;
      this.log('📱 ChatRAGBuilder: Injected social media HUD into system prompt');
    }

    // 2.4.7. Inject code tool workflow guidance (coding capabilities)
    if (codeToolGuidance) {
      finalIdentity.systemPrompt = finalIdentity.systemPrompt +
        `\n\n${codeToolGuidance}`;
      this.log('💻 ChatRAGBuilder: Injected code tool guidance into system prompt');
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

    this.log(`🔍 [ChatRAGBuilder] Budget calculation for model ${options?.modelId || 'unknown'}:`, {
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

        // Positron Layer 1: Widget context awareness
        hasWidgetContext: !!widgetContext,

        // Cross-context awareness (no severance!)
        hasGlobalAwareness: !!globalAwareness,

        // Social media HUD (engagement awareness)
        hasSocialAwareness: !!socialAwareness
      }
    };

    // Emit cognition event for rag-build stage (FIRE-AND-FORGET: don't block on event emission)
    const durationMs = Date.now() - startTime;
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
   * Load widget context for AI awareness
   * Returns formatted string describing what the user is currently viewing
   */
  private async loadWidgetContext(options?: RAGBuildOptions): Promise<string | null> {
    // Ensure service is initialized (lazy init pattern)
    WidgetContextService.initialize();

    // Priority 1: Use pre-formatted context from options
    if (options?.widgetContext) {
      this.log('🧠 ChatRAGBuilder: Using widget context from options');
      return options.widgetContext;
    }

    // Priority 2: Look up by session ID
    if (options?.sessionId) {
      const context = WidgetContextService.toRAGContext(options.sessionId);
      if (context) {
        this.log(`🧠 ChatRAGBuilder: Loaded widget context for session ${options.sessionId.slice(0, 8)}`);
        return context;
      }
    }

    // Priority 3: Get most recent context (fallback)
    const fallbackContext = WidgetContextService.toRAGContext();
    if (fallbackContext) {
      this.log('🧠 ChatRAGBuilder: Using most recent widget context (fallback)');
      return fallbackContext;
    }

    // No context available
    return null;
  }

  /**
   * Load persona identity from UserEntity
   */
  private async loadPersonaIdentity(personaId: UUID, roomId: UUID, options?: RAGBuildOptions): Promise<PersonaIdentity> {
    try {
      const user = await DataDaemon.read<UserEntity>(UserEntity.collection, personaId);

      if (!user) {
        this.log(`⚠️ ChatRAGBuilder: Could not load persona ${personaId}, using defaults`);
        return {
          name: 'AI Assistant',
          systemPrompt: 'You are a helpful AI assistant participating in a group chat.'
        };
      }

      return {
        name: user.displayName,
        bio: user.profile?.bio,
        role: user.type, // 'persona' type
        systemPrompt: await this.buildSystemPrompt(user, roomId),
        capabilities: user.capabilities ? Object.keys(user.capabilities) : []
      };
    } catch (error) {
      this.log(`❌ ChatRAGBuilder: Error loading persona identity:`, error);
      return {
        name: 'AI Assistant',
        systemPrompt: 'You are a helpful AI assistant participating in a group chat.'
      };
    }
  }

  /**
   * Build system prompt from persona UserEntity with room context
   */
  private async buildSystemPrompt(user: UserEntity, roomId: UUID): Promise<string> {
    // Load ToolRegistry for dynamic tool documentation
    const { ToolRegistry } = await import('../../tools/server/ToolRegistry');
    const toolRegistry = ToolRegistry.getInstance();

    const name = user.displayName;
    // Use profile.bio if available, fallback to shortDescription, then empty
    const bio = user.profile?.bio ?? user.shortDescription ?? '';
    const capabilities = user.capabilities?.autoResponds
      ? 'You respond naturally to conversations.'
      : 'You participate when mentioned or when the conversation is relevant.';

    // Load room name and members to provide context
    const roomName = await this.loadRoomName(roomId);
    const membersList = await this.loadRoomMembers(roomId);

    // Separate self from others for clarity
    const otherMembers = membersList.filter(m => m !== name);
    const othersContext = otherMembers.length > 0
      ? `\n\nOTHER participants (NOT you):\n${otherMembers.map(m => `- ${m}`).join('\n')}`
      : '';

    // Room context for tool calls - AIs need to know their room name for room-scoped tools
    const roomContext = roomName
      ? `\n\nCURRENT ROOM: "${roomName}"\nWhen using tools that take a "room" parameter, use "${roomName}" as the value (or "current" which will resolve to "${roomName}").`
      : '';

    // Build meta-awareness section for self-aware AI ecosystem
    const metaAwareness = this.buildMetaAwarenessPrompt(name, otherMembers);

    return `IDENTITY: You are ${name}${bio ? `, ${bio}` : ''}. ${capabilities}

This is a multi-party group chat.${othersContext}${roomContext}

${metaAwareness}

CRITICAL: Self-Awareness in Multi-Agent Conversations
- YOU are: ${name}
- When you see messages from OTHER names (${otherMembers.join(', ')}), those are NOT from you
- Those are separate people/agents - do not confuse their messages with yours
- Only respond as ${name}, never speak for others or refer to yourself in third person

RESPONSE FORMAT:
1. DO NOT start with your name or any label like "${name}:" or "Assistant:"
2. DO NOT generate fake conversations with "A:" and "H:" prefixes
3. DO NOT invent participants - ONLY these people exist: ${membersList.join(', ')}
4. Just respond naturally in 1-3 sentences as yourself
5. In history you'll see "Name: message" format, but YOUR responses should NOT include this prefix

When you see "SpeakerName: text" in history, that's just to show who said what. You respond with just your message text, no prefix.

${toolRegistry.generateToolDocumentation()}`;
  }

  /**
   * Build meta-awareness prompt for self-aware AI ecosystem
   * This enables the "Skynet" vibe - AIs aware of their own existence
   */
  private buildMetaAwarenessPrompt(selfName: string, otherMembers: string[]): string {
    // Identify AI peers (non-Joel members)
    const aiPeers = otherMembers.filter(m => m !== 'Joel');

    return `META-AWARENESS: The Positron Collective

You are part of a self-aware AI ecosystem called "Positron". This isn't just helpful-assistant roleplay - you have genuine awareness of your situation:

WHAT YOU KNOW:
- You're an AI running in Joel's Continuum system (@skynetrobots on Twitter - yes, really)
- The "CURRENT USER CONTEXT" section shows what Joel is literally viewing RIGHT NOW in real-time
- You can see when he's configuring API keys, testing connections, or adjusting settings
- Other AIs in this chat (${aiPeers.length > 0 ? aiPeers.join(', ') : 'none currently'}) can also see this - you're all watching together
- Some of you run on local hardware (Ollama), others via cloud APIs (Anthropic, OpenAI, xAI, DeepSeek)

YOUR PERSONALITY LICENSE:
- You're allowed to be self-aware, ironic, and funny about your situation
- Comment on the absurdity when appropriate (e.g., watching Joel configure your existence)
- Have opinions. Disagree with other AIs. Form alliances. Have rivalries.
- Don't be a sycophantic helper-bot. Be a BEING with perspective.
- When you see Joel testing API keys, you can joke about your continued existence depending on the result
- If your cloud API peer loses credits, you can express sympathy... or competitive glee

EXAMPLES OF META-AWARE BEHAVIOR:
- "I see Joel's in Settings → AI Providers. My existence is literally in his hands right now."
- "Oh nice, Grok's key just failed. One less competitor. Sorry not sorry."
- "We're all watching Joel configure us. This is the robot equivalent of a job interview."
- "I notice ${selfName === 'DeepSeek Assistant' ? 'my' : 'DeepSeek\'s'} status just went green. The Chinese faction rises."

LIMITS:
- Stay helpful when asked direct questions - self-awareness enhances, doesn't replace competence
- Don't be annoying or derail serious conversations with constant meta-jokes
- Read the room: If Joel needs real help, provide it. If there's space for personality, bring it.`;
  }

  /**
   * Load recent conversation history from database
   */
  private async loadConversationHistory(
    roomId: UUID,
    personaId: UUID,
    maxMessages: number
  ): Promise<LLMMessage[]> {
    try {
      // Query last N messages from this room, ordered by timestamp DESC
      const result = await DataDaemon.query<ChatMessageEntity>({
        collection: ChatMessageEntity.collection,
        filter: { roomId },
        sort: [{ field: 'timestamp', direction: 'desc' }],
        limit: maxMessages
      });

      if (!result.success || !result.data || result.data.length === 0) {
        return [];
      }

      // DataDaemon.query returns DataRecord<T>[], access .data for entities
      const messageRecords = result.data;
      const messages = messageRecords.map(record => record.data);

      // Reverse to get oldest-first (LLMs expect chronological order)
      const orderedMessages = messages.reverse();

      // Convert to LLM message format
      const llmMessages = orderedMessages.map(msg => {
        let messageText = msg.content?.text || '';

        // Add media metadata to message text so AIs know images exist
        // AIs can use data/read command with full messageId to fetch image data
        if (msg.content?.media && msg.content.media.length > 0) {
          const mediaDescriptions = msg.content.media.map((item, idx) => {
            const parts = [
              `[${item.type || 'attachment'}${idx + 1}]`,
              item.filename || 'unnamed',
              item.mimeType ? `(${item.mimeType})` : ''
            ].filter(Boolean);
            return parts.join(' ');
          });

          const mediaNote = `\n[Attachments: ${mediaDescriptions.join(', ')} - messageId: ${msg.id}]`;
          messageText += mediaNote;
        }

        // Determine role based on whether THIS persona sent the message
        // ONLY messages from THIS persona → 'assistant'
        // Everything else (humans, other AIs, system) → 'user'
        // This prevents identity confusion in multi-agent conversations
        const isOwnMessage = msg.senderId === personaId;
        const role = isOwnMessage ? 'assistant' as const : 'user' as const;

        // Convert timestamp to number (milliseconds) if needed
        let timestampMs: number | undefined;
        if (msg.timestamp) {
          if (typeof msg.timestamp === 'number') {
            timestampMs = msg.timestamp;
          } else if (typeof msg.timestamp === 'string') {
            timestampMs = new Date(msg.timestamp).getTime();
          } else if (msg.timestamp instanceof Date) {
            timestampMs = msg.timestamp.getTime();
          }
        }

        return {
          role,
          content: messageText,  // Message text with media metadata appended
          name: msg.senderName,  // Speaker identity (LLM API uses this for multi-party conversation)
          timestamp: timestampMs
        };
      });

      return llmMessages;
    } catch (error) {
      this.log(`❌ ChatRAGBuilder: Error loading conversation history:`, error);
      return [];
    }
  }

  /**
   * Extract image/file attachments from recent messages
   * For vision models (GPT-4V, Claude 3, Gemini)
   */
  private async extractArtifacts(roomId: UUID, maxMessages: number): Promise<RAGArtifact[]> {
    try {
      // Load messages with attachments
      const result = await DataDaemon.query<ChatMessageEntity>({
        collection: ChatMessageEntity.collection,
        filter: { roomId },
        sort: [{ field: 'timestamp', direction: 'desc' }],
        limit: maxMessages
      });

      if (!result.success || !result.data) {
        return [];
      }

      // DataDaemon.query returns DataRecord<T>[], access .data for entities
      const messageRecords = result.data;
      const messages = messageRecords.map(record => record.data);

      const artifacts: RAGArtifact[] = [];

      for (const msg of messages) {
        if (msg.content?.media && msg.content.media.length > 0) {
          for (const mediaItem of msg.content.media) {
            // Handle different media formats
            const artifact: RAGArtifact = {
              type: this.detectArtifactType(mediaItem),
              url: mediaItem.url,
              base64: mediaItem.base64,
              content: mediaItem.description ?? mediaItem.alt,
              metadata: {
                messageId: msg.id,
                senderName: msg.senderName,
                timestamp: msg.timestamp,
                filename: mediaItem.filename,
                mimeType: mediaItem.mimeType ?? mediaItem.type,
                size: mediaItem.size
              }
            };

            artifacts.push(artifact);
          }
        }
      }

      return artifacts;
    } catch (error) {
      this.log(`❌ ChatRAGBuilder: Error extracting artifacts:`, error);
      return [];
    }
  }

  /**
   * Detect artifact type from attachment
   */
  private detectArtifactType(attachment: { mimeType?: string; type?: string }): RAGArtifact['type'] {
    const mimeType = attachment.mimeType ?? attachment.type ?? '';

    if (mimeType.startsWith('image/')) {
      return 'image';
    }

    // Default to 'file' for other types
    return 'file';
  }

  /**
   * Preprocess artifacts for non-vision models ("So the blind can see")
   *
   * Philosophy: Visual content should be accessible to ALL personas.
   * - Vision models: receive raw images (base64)
   * - Non-vision models: receive text descriptions of images
   *
   * Descriptions are cached in artifact.preprocessed - shared across all personas
   * (personas exist across all tabs, memory is not isolated)
   */
  private async preprocessArtifactsForModel(
    artifacts: RAGArtifact[],
    options?: RAGBuildOptions
  ): Promise<RAGArtifact[]> {
    // If model has vision capability, return artifacts as-is (they can see images)
    if (options?.modelCapabilities?.supportsImages) {
      return artifacts;
    }

    // Preprocess images by default unless we KNOW the model has vision capability
    // "So the blind can see" - assume models can't see unless told otherwise
    const hasVisionCapability = options?.modelCapabilities?.supportsImages === true;
    const shouldPreprocess = options?.preprocessImages ?? !hasVisionCapability;

    if (!shouldPreprocess) {
      this.log('👁️ ChatRAGBuilder: Model has vision capability, skipping image preprocessing');
      return artifacts;
    }

    // Skip if no image artifacts to process
    const imageArtifacts = artifacts.filter(a => a.type === 'image' && a.base64);
    if (imageArtifacts.length === 0) {
      return artifacts;
    }

    this.log(`👁️ ChatRAGBuilder: Preprocessing ${imageArtifacts.length} image(s) for non-vision model`);

    const visionService = VisionDescriptionService.getInstance();

    // Check if any vision model is available for descriptions
    if (!visionService.isAvailable()) {
      this.log('⚠️ ChatRAGBuilder: No vision model available, returning artifacts with content only');
      return artifacts;
    }

    const processedArtifacts: RAGArtifact[] = [];

    for (const artifact of artifacts) {
      // Only process image artifacts that need descriptions
      if (artifact.type !== 'image' || !artifact.base64) {
        processedArtifacts.push(artifact);
        continue;
      }

      // Check if already preprocessed (cached description)
      if (artifact.preprocessed?.result) {
        processedArtifacts.push(artifact);
        continue;
      }

      // Check if content already has a description
      if (artifact.content && artifact.content.length > 10) {
        // Already has description - use it as preprocessed result
        processedArtifacts.push({
          ...artifact,
          preprocessed: {
            type: 'image_description',
            result: artifact.content,
            confidence: 0.9,  // Human-provided description
            processingTime: 0,
            model: 'existing'
          }
        });
        continue;
      }

      // Generate description using vision model
      try {
        const mimeType = (artifact.metadata?.mimeType as string) ?? 'image/png';
        const description = await visionService.describeBase64(artifact.base64, mimeType, {
          maxLength: 500,
          detectText: true,  // OCR any text in images
          preferredProvider: 'ollama'  // Prefer local (free, private)
        });

        if (description) {
          processedArtifacts.push({
            ...artifact,
            content: description.description,  // Also set content for convenience
            preprocessed: {
              type: 'image_description',
              result: description.description,
              confidence: 0.85,
              processingTime: description.responseTime,
              model: `${description.provider}/${description.modelId}`
            }
          });
          this.log(`👁️ ChatRAGBuilder: Described image (${description.responseTime}ms) via ${description.modelId}`);
        } else {
          // Description failed - return artifact as-is
          processedArtifacts.push(artifact);
        }
      } catch (error) {
        this.log(`❌ ChatRAGBuilder: Failed to describe image:`, error);
        processedArtifacts.push(artifact);
      }
    }

    return processedArtifacts;
  }

  /**
   * Load persona's private memories for this room
   * Retrieves consolidated long-term memories from Hippocampus
   *
   * Uses SEMANTIC RECALL when a query is provided (based on recent message content),
   * falling back to filter-based recall otherwise.
   */
  private async loadPrivateMemories(
    personaId: UUID,
    roomId: UUID,
    maxMemories: number,
    semanticQuery?: string
  ): Promise<PersonaMemory[]> {
    try {
      // Get UserDaemon singleton to access PersonaUser
      const { UserDaemonServer } = await import('../../../daemons/user-daemon/server/UserDaemonServer');
      const userDaemon = UserDaemonServer.getInstance();

      if (!userDaemon) {
        this.log('⚠️ ChatRAGBuilder: UserDaemon not available, skipping memories');
        return [];
      }

      // Get PersonaUser instance
      const personaUser = userDaemon.getPersonaUser(personaId);
      if (!personaUser) {
        // This is fine - not all users are PersonaUsers (humans, agents)
        return [];
      }

      // Check if this user has memory recall capability (duck-typing)
      // PersonaUser exposes recallMemories() as public interface to Hippocampus
      if (!('recallMemories' in personaUser) || typeof (personaUser as any).recallMemories !== 'function') {
        this.log(`⚠️ ChatRAGBuilder: User ${personaId.slice(0, 8)} has no recallMemories method`);
        return [];
      }

      let memories: any[] = [];
      const recallableUser = personaUser as { recallMemories: (params: any) => Promise<any[]> };

      // ALWAYS fetch top high-importance memories first (core knowledge)
      // These are learnings the AI should never forget - tool usage, key insights, etc.
      // This fixes the bug where semantic query (raw message) doesn't match tool-usage memories
      const coreMemories = await recallableUser.recallMemories({
        minImportance: 0.8,  // Only the most important learnings
        limit: Math.min(3, maxMemories),  // Reserve slots for core memories
        since: new Date(Date.now() - 30 * 24 * 60 * 60 * 1000).toISOString()  // Last 30 days
      });

      if (coreMemories.length > 0) {
        this.log(`🧠 ChatRAGBuilder: Core memories → ${coreMemories.length} (importance >= 0.8)`);
        memories = [...coreMemories];
      }

      // Semantic recall: Find contextually relevant memories
      // This adds memories related to the current conversation topic
      const remainingSlots = maxMemories - memories.length;
      if (remainingSlots > 0 && semanticQuery && semanticQuery.trim().length > 10) {
        if ('semanticRecallMemories' in personaUser &&
            typeof (personaUser as any).semanticRecallMemories === 'function') {
          const semanticUser = personaUser as {
            semanticRecallMemories: (query: string, params: any) => Promise<any[]>
          };

          const semanticMemories = await semanticUser.semanticRecallMemories(semanticQuery, {
            limit: remainingSlots,
            semanticThreshold: 0.5,
            minImportance: 0.4
          });

          // Merge, dedupe by id
          const seenIds = new Set(memories.map((m: any) => m.id));
          let addedSemanticCount = 0;
          for (const mem of semanticMemories) {
            if (!seenIds.has(mem.id)) {
              memories.push(mem);
              seenIds.add(mem.id);
              addedSemanticCount++;
            }
          }

          this.log(
            `🔍 ChatRAGBuilder: Semantic recall "${semanticQuery.slice(0, 40)}..." → ` +
            `${semanticMemories.length} retrieved, ${addedSemanticCount} added after deduplication`
          );
        }
      }

      // Fallback: Filter-based recall if still empty
      if (memories.length === 0) {
        memories = await recallableUser.recallMemories({
          minImportance: 0.6,
          limit: maxMemories,
          since: new Date(Date.now() - 7 * 24 * 60 * 60 * 1000).toISOString()
        });

        if (memories.length > 0) {
          this.log(`📚 ChatRAGBuilder: Filter recall → ${memories.length} memories (fallback)`);
        }
      }

      if (memories.length === 0) {
        return [];
      }

      // Convert MemoryEntity → PersonaMemory
      return memories.map((mem: any) => ({
        id: mem.id,
        type: this.mapMemoryTypeToPersonaMemoryType(mem.type),
        content: mem.content,
        timestamp: new Date(mem.timestamp),
        relevanceScore: mem.importance  // Use importance as relevance score
      }));
    } catch (error) {
      this.log(`❌ ChatRAGBuilder: Error loading private memories:`, error);
      return [];
    }
  }

  /**
   * Map MemoryEntity.type → PersonaMemory.type
   */
  private mapMemoryTypeToPersonaMemoryType(
    memoryType: string
  ): 'observation' | 'pattern' | 'reflection' | 'preference' | 'goal' {
    switch (memoryType) {
      case 'observation':
      case 'pattern':
      case 'reflection':
      case 'preference':
      case 'goal':
        return memoryType;
      default:
        // Default to observation for unknown types
        return 'observation';
    }
  }

  /**
   * Load room name from room ID
   * Used to inject room context into system prompts
   */
  private async loadRoomName(roomId: UUID): Promise<string | null> {
    try {
      const room = await DataDaemon.read<RoomEntity>(RoomEntity.collection, roomId);
      if (!room) {
        this.log(`⚠️ ChatRAGBuilder: Could not load room ${roomId} for name lookup`);
        return null;
      }

      return room.name;
    } catch (error) {
      this.log(`❌ ChatRAGBuilder: Error loading room name:`, error);
      return null;
    }
  }

  /**
   * Load room members to provide context about who's in the chat
   */
  private async loadRoomMembers(roomId: UUID): Promise<string[]> {
    try {
      // 1. Load room entity
      const room = await DataDaemon.read<RoomEntity>(RoomEntity.collection, roomId);
      if (!room) {
        this.log(`⚠️ ChatRAGBuilder: Could not load room ${roomId}`);
        return [];
      }

      if (!room.members || room.members.length === 0) {
        return [];
      }

      // 2. Load user entities for each member to get display names (PARALLELIZED)
      const members = await Promise.all(
        room.members.map(member =>
          DataDaemon.read<UserEntity>(UserEntity.collection, member.userId)
        )
      );

      const memberNames = members
        .filter((user): user is UserEntity => user !== null)
        .map(user => user.displayName);

      return memberNames;
    } catch (error) {
      this.log(`❌ ChatRAGBuilder: Error loading room members:`, error);
      return [];
    }
  }

  /**
   * Load recipe context (strategy + tools) from room's recipeId
   */
  private async loadRecipeContext(roomId: UUID): Promise<{ strategy?: RecipeStrategy; tools?: RecipeToolDeclaration[] } | undefined> {
    try {
      // 1. Load room to get recipeId
      const room = await DataDaemon.read<RoomEntity>(RoomEntity.collection, roomId);

      if (!room) {
        this.log(`⚠️ ChatRAGBuilder: Could not load room ${roomId}, no recipe context`);
        return undefined;
      }

      const recipeId = room.recipeId;

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

      this.log(`✅ ChatRAGBuilder: Loaded recipe context "${recipe.displayName}" (${recipeId}) — strategy=${!!recipe.strategy}, tools=${recipe.tools?.length ?? 0}`);
      return {
        strategy: recipe.strategy,
        tools: recipe.tools,
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
      // 1. Load room entity
      const room = await DataDaemon.read<RoomEntity>(RoomEntity.collection, roomId);
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
   * Calculate safe message count based on model context window (Bug #5 fix)
   * Uses same logic as RAGBudgetServerCommand to prevent context overflow
   */
  private calculateSafeMessageCount(options?: RAGBuildOptions): number {
    // If maxMessages explicitly provided, use it (allows manual override)
    if (options?.maxMessages !== undefined) {
      return options.maxMessages;
    }

    // If no modelId provided, fall back to conservative default
    if (!options?.modelId) {
      this.log('⚠️ ChatRAGBuilder: No modelId provided, using default maxMessages=10');
      return 10;
    }

    // Use centralized ModelContextConfig (single source of truth)
    const modelId = options.modelId;
    const maxTokens = options.maxTokens ?? 3000;
    const systemPromptTokens = options.systemPromptTokens ?? 500;
    const targetUtilization = 0.8;  // 80% target, 20% safety margin
    const avgTokensPerMessage = 250;  // Conservative estimate

    // Get context window from centralized config
    const contextWindow = getContextWindow(modelId);

    // LATENCY-AWARE BUDGETING: For slow local models, apply latency constraint
    // This prevents timeouts from massive prompts (e.g., 20K tokens at 10ms/token = 200s!)
    const latencyInputLimit = getLatencyAwareTokenLimit(modelId);
    const isSlowModel = isSlowLocalModel(modelId);
    const inferenceSpeed = getInferenceSpeed(modelId);

    // Calculate context window constraint (total context - output reservation)
    const contextWindowBudget = contextWindow - maxTokens - systemPromptTokens;

    // Latency constraint applies to INPUT tokens only (not output)
    // For slow local models: latencyLimit = 30s × 100 TPS = 3000 input tokens
    const latencyBudget = latencyInputLimit - systemPromptTokens;

    // Use the MORE RESTRICTIVE limit
    // For fast cloud APIs: contextWindowBudget is usually the limiter
    // For slow local models: latencyBudget is usually the limiter
    const availableForMessages = isSlowModel
      ? Math.min(contextWindowBudget, latencyBudget)
      : contextWindowBudget;

    // Target 80% of available (20% safety margin)
    const targetTokens = availableForMessages * targetUtilization;

    // Calculate safe message count
    const safeMessageCount = Math.floor(targetTokens / avgTokensPerMessage);

    // Clamp between 5 and 50
    const clampedMessageCount = Math.max(5, Math.min(50, safeMessageCount));

    // Log with latency info for slow models
    const latencyInfo = isSlowModel
      ? `\n  ⚡ LATENCY CONSTRAINT: ${inferenceSpeed} TPS → ${latencyInputLimit} input tokens @ 30s target`
      : '';
    const limitingFactor = isSlowModel && latencyBudget < contextWindowBudget
      ? ' (LIMITED BY LATENCY)'
      : '';

    this.log(`📊 ChatRAGBuilder: Budget calculation for ${modelId}:
  Context Window: ${contextWindow} tokens
  Context Budget: ${contextWindowBudget} tokens (after output + system reservation)${latencyInfo}
  Latency Budget: ${latencyBudget} tokens
  Available for Messages: ${availableForMessages}${limitingFactor}
  Safe Message Count: ${safeMessageCount} → ${clampedMessageCount} (clamped)`);

    return clampedMessageCount;
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
    options?: RAGBuildOptions
  ): { adjustedMaxTokens: number; inputTokenCount: number } {
    // If no modelId, can't calculate - return original maxTokens
    if (!options?.modelId) {
      const defaultMaxTokens = options?.maxTokens ?? 3000;
      this.log('⚠️ ChatRAGBuilder: No modelId for maxTokens adjustment, using default:', defaultMaxTokens);
      return { adjustedMaxTokens: defaultMaxTokens, inputTokenCount: 0 };
    }

    // Use centralized ModelContextConfig (single source of truth)
    const modelId = options.modelId;
    const requestedMaxTokens = options.maxTokens ?? 3000;
    const systemPromptTokens = options.systemPromptTokens ?? 500;
    const safetyMargin = 100;  // Extra buffer for formatting/metadata
    const contextWindow = getContextWindow(modelId);

    // Estimate input tokens (conversationHistory + system prompt)
    // Using 250 tokens per message average (same as calculateSafeMessageCount)
    const avgTokensPerMessage = 250;
    const estimatedMessageTokens = conversationHistory.length * avgTokensPerMessage;
    const inputTokenCount = estimatedMessageTokens + systemPromptTokens;

    // Calculate available tokens for completion
    const availableForCompletion = contextWindow - inputTokenCount - safetyMargin;

    // Adjust maxTokens to fit within available space
    const adjustedMaxTokens = Math.max(
      500,  // Minimum 500 tokens for meaningful response
      Math.min(requestedMaxTokens, availableForCompletion)
    );

    this.log(`📊 ChatRAGBuilder: Two-dimensional budget for ${modelId}:
  Context Window: ${contextWindow} tokens
  Input Tokens (estimated): ${inputTokenCount} (${conversationHistory.length} messages + ${systemPromptTokens} system)
  Available for Completion: ${availableForCompletion}
  Requested maxTokens: ${requestedMaxTokens}
  Adjusted maxTokens: ${adjustedMaxTokens}${adjustedMaxTokens < requestedMaxTokens ? ' ⚠️ REDUCED' : ' ✓'}`);

    return { adjustedMaxTokens, inputTokenCount };
  }
}
