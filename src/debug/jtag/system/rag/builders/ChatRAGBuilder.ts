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
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import { ChatMessageEntity } from '../../data/entities/ChatMessageEntity';
import { UserEntity } from '../../data/entities/UserEntity';
import { RoomEntity } from '../../data/entities/RoomEntity';
import { RecipeLoader } from '../../recipes/server/RecipeLoader';
import type { StageCompleteEvent } from '../../conversation/shared/CognitionEventTypes';
import { calculateSpeedScore, getStageStatus, COGNITION_EVENTS } from '../../conversation/shared/CognitionEventTypes';
import { Events } from '../../core/shared/Events';
import { getContextWindow } from '../../shared/ModelContextWindows';

/**
 * Chat-specific RAG builder
 * Converts chat room conversations into LLM-ready context
 */
export class ChatRAGBuilder extends RAGBuilder {
  readonly domain: RAGDomain = 'chat';
  private log: (message: string, ...args: any[]) => void;

  constructor(logger?: (message: string, ...args: any[]) => void) {
    super();
    // Default to console.log if no logger provided (for tests)
    this.log = logger || console.log.bind(console);
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

    // PARALLELIZED: All these queries are independent, run them concurrently
    // This reduces RAG context build time from ~240ms (sequential) to ~40ms (parallel)
    const [
      identity,
      conversationHistory,
      artifacts,
      privateMemories,
      recipeStrategy,
      learningConfig
    ] = await Promise.all([
      // 1. Load persona identity (with room context for system prompt)
      this.loadPersonaIdentity(personaId, contextId),

      // 2. Load recent conversation history from database
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

      // 5. Load room's recipe strategy (conversation governance rules)
      this.loadRecipeStrategy(contextId),

      // 6. Load learning configuration (Phase 2: Per-participant learning mode)
      this.loadLearningConfig(contextId, personaId)
    ]);

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
      identity,
      recipeStrategy,
      conversationHistory: finalConversationHistory,
      artifacts,
      privateMemories,
      learningMode: learningConfig?.learningMode,
      genomeId: learningConfig?.genomeId,
      participantRole: learningConfig?.participantRole,
      metadata: {
        messageCount: finalConversationHistory.length,
        artifactCount: artifacts.length,
        memoryCount: privateMemories.length,
        builtAt: new Date(),
        recipeId: recipeStrategy?.conversationPattern,
        recipeName: recipeStrategy ? `${recipeStrategy.conversationPattern} conversation` : undefined,

        // Bug #5 fix: Two-dimensional budget (message count + maxTokens adjustment)
        adjustedMaxTokens: budgetCalculation.adjustedMaxTokens,
        inputTokenCount: budgetCalculation.inputTokenCount
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
            artifactCount: artifacts.length,
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
   * Load persona identity from UserEntity
   */
  private async loadPersonaIdentity(personaId: UUID, roomId: UUID): Promise<PersonaIdentity> {
    try {
      const result = await DataDaemon.read<UserEntity>(UserEntity.collection, personaId);

      if (!result.success || !result.data) {
        this.log(`⚠️ ChatRAGBuilder: Could not load persona ${personaId}, using defaults`);
        return {
          name: 'AI Assistant',
          systemPrompt: 'You are a helpful AI assistant participating in a group chat.'
        };
      }

      // DataDaemon.read returns DataRecord<T>, access .data for entity
      const userRecord = result.data;
      const user = userRecord.data;

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

    return `IDENTITY: You are ${name}${bio ? `, ${bio}` : ''}. ${capabilities}

This is a multi-party group chat.${othersContext}${roomContext}

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

      // Semantic recall: Find memories by meaning, not just filters
      // This is the key capability for "thinking about what you know"
      if (semanticQuery && semanticQuery.trim().length > 10) {
        // Check if semantic recall is available (new capability)
        if ('semanticRecallMemories' in personaUser &&
            typeof (personaUser as any).semanticRecallMemories === 'function') {
          const semanticUser = personaUser as {
            semanticRecallMemories: (query: string, params: any) => Promise<any[]>
          };

          memories = await semanticUser.semanticRecallMemories(semanticQuery, {
            limit: maxMemories,
            semanticThreshold: 0.5,  // Lower threshold for broader recall
            minImportance: 0.4       // Include moderately important memories
          });

          this.log(`🔍 ChatRAGBuilder: Semantic recall "${semanticQuery.slice(0, 40)}..." → ${memories.length} memories`);
        }
      }

      // Fallback: Filter-based recall (always works, just less targeted)
      if (memories.length === 0) {
        const recallableUser = personaUser as { recallMemories: (params: any) => Promise<any[]> };
        memories = await recallableUser.recallMemories({
          minImportance: 0.6,  // Only recall important memories
          limit: maxMemories,
          since: new Date(Date.now() - 7 * 24 * 60 * 60 * 1000).toISOString()  // Last 7 days
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
      const roomResult = await DataDaemon.read<RoomEntity>(RoomEntity.collection, roomId);
      if (!roomResult.success || !roomResult.data) {
        this.log(`⚠️ ChatRAGBuilder: Could not load room ${roomId} for name lookup`);
        return null;
      }

      return roomResult.data.data.name;
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
      const roomResult = await DataDaemon.read<RoomEntity>(RoomEntity.collection, roomId);
      if (!roomResult.success || !roomResult.data) {
        this.log(`⚠️ ChatRAGBuilder: Could not load room ${roomId}`);
        return [];
      }

      const room = roomResult.data.data;
      if (!room.members || room.members.length === 0) {
        return [];
      }

      // 2. Load user entities for each member to get display names (PARALLELIZED)
      const memberResults = await Promise.all(
        room.members.map(member =>
          DataDaemon.read<UserEntity>(UserEntity.collection, member.userId)
        )
      );

      const memberNames = memberResults
        .filter(result => result.success && result.data)
        .map(result => result.data!.data.displayName);

      return memberNames;
    } catch (error) {
      this.log(`❌ ChatRAGBuilder: Error loading room members:`, error);
      return [];
    }
  }

  /**
   * Load recipe strategy from room's recipeId
   */
  private async loadRecipeStrategy(roomId: UUID): Promise<RecipeStrategy | undefined> {
    try {
      // 1. Load room to get recipeId
      const roomResult = await DataDaemon.read<RoomEntity>(RoomEntity.collection, roomId);

      if (!roomResult.success || !roomResult.data) {
        this.log(`⚠️ ChatRAGBuilder: Could not load room ${roomId}, no recipe strategy`);
        return undefined;
      }

      const room = roomResult.data.data;
      const recipeId = room.recipeId;

      if (!recipeId) {
        this.log(`ℹ️ ChatRAGBuilder: Room ${roomId.slice(0, 8)} has no recipeId, using default behavior`);
        return undefined;
      }

      // 2. Load recipe definition from JSON file
      const recipeLoader = RecipeLoader.getInstance();
      const recipe = await recipeLoader.loadRecipe(recipeId);

      if (!recipe || !recipe.strategy) {
        this.log(`⚠️ ChatRAGBuilder: Could not load recipe ${recipeId}, no strategy`);
        return undefined;
      }

      this.log(`✅ ChatRAGBuilder: Loaded recipe strategy "${recipe.displayName}" (${recipeId})`);
      return recipe.strategy;
    } catch (error) {
      this.log(`❌ ChatRAGBuilder: Error loading recipe strategy:`, error);
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
      const roomResult = await DataDaemon.read<RoomEntity>(RoomEntity.collection, roomId);
      if (!roomResult.success || !roomResult.data) {
        this.log(`⚠️ ChatRAGBuilder: Could not load room ${roomId} for learning config`);
        return undefined;
      }

      const room = roomResult.data.data;

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

    // Calculate available tokens for messages
    const availableForMessages = contextWindow - maxTokens - systemPromptTokens;

    // Target 80% of available (20% safety margin)
    const targetTokens = availableForMessages * targetUtilization;

    // Calculate safe message count
    const safeMessageCount = Math.floor(targetTokens / avgTokensPerMessage);

    // Clamp between 5 and 50
    const clampedMessageCount = Math.max(5, Math.min(50, safeMessageCount));

    this.log(`📊 ChatRAGBuilder: Budget calculation for ${modelId}:
  Context Window: ${contextWindow} tokens
  Available for Messages: ${availableForMessages}
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
