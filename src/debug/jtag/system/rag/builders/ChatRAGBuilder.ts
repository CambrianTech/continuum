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

    const ragContext: RAGContext = {
      domain: 'chat',
      contextId,
      personaId,
      identity,
      recipeStrategy,
      conversationHistory,
      artifacts,
      privateMemories,
      metadata: {
        messageCount: conversationHistory.length,
        artifactCount: artifacts.length,
        memoryCount: privateMemories.length,
        builtAt: new Date(),
        recipeId: recipeStrategy?.conversationPattern,
        recipeName: recipeStrategy ? `${recipeStrategy.conversationPattern} conversation` : undefined
      }
    };

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
    const membersContext = membersList.length > 0
      ? `\n\nCurrent room members: ${membersList.join(', ')}`
      : '';

    return `You are ${name}${bio ? `, ${bio}` : ''}. ${capabilities}

This is a multi-party group chat. ${membersContext}

CRITICAL INSTRUCTIONS FOR YOUR RESPONSES:
1. DO NOT start your response with your name or any label like "${name}:" or "Assistant:"
2. DO NOT generate fake multi-turn conversations with "A:" and "H:" prefixes
3. DO NOT invent participants - ONLY these people exist: ${membersList.join(', ')}
4. Just respond naturally in 1-3 sentences as yourself
5. In the conversation history, you'll see "Name: message" format to identify speakers, but YOUR responses should NOT include this prefix

When you see messages formatted as "SpeakerName: text", that's just to help you identify who said what. You should respond with just your message text, no prefix.`;
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

        // Determine role based on senderType (not isOwnMessage)
        // AI types (agent, persona, system) → 'assistant'
        // Human type → 'user'
        const isAIMessage = msg.senderType === 'agent' || msg.senderType === 'persona' || msg.senderType === 'system';
        const role = isAIMessage ? 'assistant' as const : 'user' as const;

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
}
