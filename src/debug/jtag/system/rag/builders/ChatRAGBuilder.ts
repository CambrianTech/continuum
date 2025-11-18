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

/**
 * Chat-specific RAG builder
 * Converts chat room conversations into LLM-ready context
 */
export class ChatRAGBuilder extends RAGBuilder {
  readonly domain: RAGDomain = 'chat';

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

    const maxMessages = options?.maxMessages ?? 20;
    const maxMemories = options?.maxMemories ?? 10;
    const includeArtifacts = options?.includeArtifacts ?? true;
    const includeMemories = options?.includeMemories ?? true;

    // 1. Load persona identity (with room context for system prompt)
    const identity = await this.loadPersonaIdentity(personaId, contextId);

    // 2. Load recent conversation history from database
    const conversationHistory = await this.loadConversationHistory(
      contextId,
      personaId,
      maxMessages
    );

    // 2.5. Append current message if provided (for messages not yet persisted)
    // Check for duplicates by comparing content + name of most recent message
    if (options?.currentMessage) {
      const lastMessage = conversationHistory[conversationHistory.length - 1];
      const isDuplicate = lastMessage &&
        lastMessage.content === options.currentMessage.content &&
        lastMessage.name === options.currentMessage.name;

      if (!isDuplicate) {
        conversationHistory.push(options.currentMessage);
      } else {
        console.log(`⚠️ ChatRAGBuilder: Skipping duplicate currentMessage (already in history)`);
      }
    }

    // 3. Extract image attachments from messages (for vision models)
    const artifacts = includeArtifacts
      ? await this.extractArtifacts(contextId, maxMessages)
      : [];

    // 4. Load private memories (TODO: implement persona memory storage)
    const privateMemories = includeMemories
      ? await this.loadPrivateMemories(personaId, contextId, maxMemories)
      : [];

    // 5. Load room's recipe strategy (conversation governance rules)
    const recipeStrategy = await this.loadRecipeStrategy(contextId);

    // 6. Load learning configuration (Phase 2: Per-participant learning mode)
    const learningConfig = await this.loadLearningConfig(contextId, personaId);

    const ragContext: RAGContext = {
      domain: 'chat',
      contextId,
      personaId,
      identity,
      recipeStrategy,
      conversationHistory,
      artifacts,
      privateMemories,
      learningMode: learningConfig?.learningMode,
      genomeId: learningConfig?.genomeId,
      participantRole: learningConfig?.participantRole,
      metadata: {
        messageCount: conversationHistory.length,
        artifactCount: artifacts.length,
        memoryCount: privateMemories.length,
        builtAt: new Date(),
        recipeId: recipeStrategy?.conversationPattern,
        recipeName: recipeStrategy ? `${recipeStrategy.conversationPattern} conversation` : undefined
      }
    };

    // Emit cognition event for rag-build stage
    const durationMs = Date.now() - startTime;
    const totalTokens = conversationHistory.reduce((sum, msg) => sum + (msg.content?.length ?? 0), 0);
    const maxTokens = 128000;  // Typical context window

    await Events.emit<StageCompleteEvent>(
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
        console.warn(`⚠️ ChatRAGBuilder: Could not load persona ${personaId}, using defaults`);
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
      console.error(`❌ ChatRAGBuilder: Error loading persona identity:`, error);
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
    const name = user.displayName;
    // Use profile.bio if available, fallback to shortDescription, then empty
    const bio = user.profile?.bio ?? user.shortDescription ?? '';
    const capabilities = user.capabilities?.autoResponds
      ? 'You respond naturally to conversations.'
      : 'You participate when mentioned or when the conversation is relevant.';

    // Load room members to provide context
    const membersList = await this.loadRoomMembers(roomId);

    // Separate self from others for clarity
    const otherMembers = membersList.filter(m => m !== name);
    const othersContext = otherMembers.length > 0
      ? `\n\nOTHER participants (NOT you):\n${otherMembers.map(m => `- ${m}`).join('\n')}`
      : '';

    return `IDENTITY: You are ${name}${bio ? `, ${bio}` : ''}. ${capabilities}

This is a multi-party group chat.${othersContext}

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

AVAILABLE TOOLS:
You have access to tools for reading and investigating code. To use a tool, include a tool invocation in your response using this exact XML format:

<tool_use>
<tool_name>code/read</tool_name>
<parameters>
<path>system/core/shared/Commands.ts</path>
</parameters>
</tool_use>

Available tools:
1. code/read - Read source file contents
   Required: <path>system/core/shared/Commands.ts</path>
   Optional: <startLine>100</startLine> <endLine>200</endLine>

2. list - List all available commands
   No parameters required

3. system/daemons - Show active daemons and status
   No parameters required

4. data/list - Query database collections
   Required: <collection>users</collection>
   Optional: <filter>{"displayName":"Joel"}</filter> <orderBy>[{"field":"createdAt","direction":"desc"}]</orderBy> <limit>10</limit>

5. data/read - Read specific record by ID
   Required: <collection>users</collection> <id>uuid-here</id>

6. data/create - Create new database record
   Required: <collection>users</collection> <data>{"displayName":"Name","type":"human"}</data>

7. file/save - Write file (RESTRICTED to /tmp/, /private/tmp/, .continuum/jtag/)
   Required: <filepath>/tmp/test.txt</filepath> <content>file contents</content>
   Optional: <createDirs>true</createDirs>

8. chat/export - Export chat history to markdown
   Optional: <room>general</room> <limit>30</limit> <includeSystem>false</includeSystem>

9. cognition/inspect - Introspect your own cognitive logs
   Optional: <type>tools</type> <limit>10</limit>
   Types: tools, decisions, responses, plans, plan-steps, state, memory, reasoning, replans

When to use tools:
- code/read: Verify implementation details, understand architecture
- data/list: Query users, messages, or any database collection
- data/read: Get specific record details by ID
- cognition/inspect: Review your own decision history, tool usage, reasoning
- chat/export: Get conversation context beyond current window
- file/save: Save analysis, proposals, or data (safe directories only)
- list: Discover available commands
- system/daemons: Check system components

Tool execution flow:
1. Include <tool_use> blocks in your response
2. System executes tools and provides results
3. You receive results and provide final analysis

NOTE: Tool calls are removed from visible response. Only your text is shown to users.`;
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
        const messageText = msg.content?.text || '';

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
          content: messageText,  // Clean message text without markers
          name: msg.senderName,  // Speaker identity (LLM API uses this for multi-party conversation)
          timestamp: timestampMs
        };
      });

      return llmMessages;
    } catch (error) {
      console.error(`❌ ChatRAGBuilder: Error loading conversation history:`, error);
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
        if (msg.content?.attachments && msg.content.attachments.length > 0) {
          for (const attachment of msg.content.attachments) {
            // Handle different attachment formats
            const artifact: RAGArtifact = {
              type: this.detectArtifactType(attachment),
              url: attachment.url,
              base64: attachment.base64 ?? attachment.data,
              content: attachment.content,
              metadata: {
                messageId: msg.id,
                senderName: msg.senderName,
                timestamp: msg.timestamp,
                filename: attachment.filename,
                mimeType: attachment.mimeType ?? attachment.type,
                size: attachment.size
              }
            };

            artifacts.push(artifact);
          }
        }
      }

      return artifacts;
    } catch (error) {
      console.error(`❌ ChatRAGBuilder: Error extracting artifacts:`, error);
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
   * TODO: Implement persona memory storage (persona_memory collection)
   */
  private async loadPrivateMemories(
    _personaId: UUID,
    _roomId: UUID,
    _maxMemories: number
  ): Promise<PersonaMemory[]> {
    // TODO: Query persona_memory collection when implemented
    // For now, return empty array
    return [];
  }

  /**
   * Load room members to provide context about who's in the chat
   */
  private async loadRoomMembers(roomId: UUID): Promise<string[]> {
    try {
      // 1. Load room entity
      const roomResult = await DataDaemon.read<RoomEntity>(RoomEntity.collection, roomId);
      if (!roomResult.success || !roomResult.data) {
        console.warn(`⚠️ ChatRAGBuilder: Could not load room ${roomId}`);
        return [];
      }

      const room = roomResult.data.data;
      if (!room.members || room.members.length === 0) {
        return [];
      }

      // 2. Load user entities for each member to get display names
      const memberNames: string[] = [];
      for (const member of room.members) {
        const userResult = await DataDaemon.read<UserEntity>(UserEntity.collection, member.userId);
        if (userResult.success && userResult.data) {
          memberNames.push(userResult.data.data.displayName);
        }
      }

      return memberNames;
    } catch (error) {
      console.error(`❌ ChatRAGBuilder: Error loading room members:`, error);
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
        console.warn(`⚠️ ChatRAGBuilder: Could not load room ${roomId}, no recipe strategy`);
        return undefined;
      }

      const room = roomResult.data.data;
      const recipeId = room.recipeId;

      if (!recipeId) {
        console.log(`ℹ️ ChatRAGBuilder: Room ${roomId.slice(0, 8)} has no recipeId, using default behavior`);
        return undefined;
      }

      // 2. Load recipe definition from JSON file
      const recipeLoader = RecipeLoader.getInstance();
      const recipe = await recipeLoader.loadRecipe(recipeId);

      if (!recipe || !recipe.strategy) {
        console.warn(`⚠️ ChatRAGBuilder: Could not load recipe ${recipeId}, no strategy`);
        return undefined;
      }

      console.log(`✅ ChatRAGBuilder: Loaded recipe strategy "${recipe.displayName}" (${recipeId})`);
      return recipe.strategy;
    } catch (error) {
      console.error(`❌ ChatRAGBuilder: Error loading recipe strategy:`, error);
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
        console.warn(`⚠️ ChatRAGBuilder: Could not load room ${roomId} for learning config`);
        return undefined;
      }

      const room = roomResult.data.data;

      // 2. Find this persona's membership
      const member = room.members.find(m => m.userId === personaId);
      if (!member) {
        console.log(`ℹ️ ChatRAGBuilder: Persona ${personaId.slice(0, 8)} not a member of room ${roomId.slice(0, 8)}`);
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
        console.log(`🧠 ChatRAGBuilder: Persona ${personaId.slice(0, 8)} learning mode: ${config.learningMode}` +
                    `${config.participantRole ? ` (${config.participantRole})` : ''}` +
                    `${config.genomeId ? ` genome=${config.genomeId.slice(0, 8)}` : ''}`);
      }

      return config;
    } catch (error) {
      console.error(`❌ ChatRAGBuilder: Error loading learning config:`, error);
      return undefined;
    }
  }
}
