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
  PersonaMemory
} from '../shared/RAGTypes';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import { ChatMessageEntity } from '../../data/entities/ChatMessageEntity';
import { UserEntity } from '../../data/entities/UserEntity';
import { RoomEntity } from '../../data/entities/RoomEntity';

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

    console.log(`📚 ChatRAGBuilder: Building context for room ${contextId.slice(0, 8)} (persona: ${personaId.slice(0, 8)})`);

    // 1. Load persona identity (with room context for system prompt)
    const identity = await this.loadPersonaIdentity(personaId, contextId);

    // 2. Load recent conversation history from database
    const conversationHistory = await this.loadConversationHistory(
      contextId,
      personaId,
      maxMessages
    );

    // 2.5. Append current message if provided (for messages not yet persisted)
    if (options?.currentMessage) {
      conversationHistory.push(options.currentMessage);
      console.log(`🔧 ChatRAGBuilder: Added current message to context (not yet in database)`);
    }

    // 3. Extract image attachments from messages (for vision models)
    const artifacts = includeArtifacts
      ? await this.extractArtifacts(contextId, maxMessages)
      : [];

    // 4. Load private memories (TODO: implement persona memory storage)
    const privateMemories = includeMemories
      ? await this.loadPrivateMemories(personaId, contextId, maxMemories)
      : [];

    const ragContext: RAGContext = {
      domain: 'chat',
      contextId,
      personaId,
      identity,
      conversationHistory,
      artifacts,
      privateMemories,
      metadata: {
        messageCount: conversationHistory.length,
        artifactCount: artifacts.length,
        memoryCount: privateMemories.length,
        builtAt: new Date()
      }
    };

    console.log(`✅ ChatRAGBuilder: Built context with ${conversationHistory.length} messages, ${artifacts.length} artifacts, ${privateMemories.length} memories`);

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
        filters: { roomId },
        sort: [{ field: 'timestamp', direction: 'desc' }],
        limit: maxMessages
      });

      if (!result.success || !result.data || result.data.length === 0) {
        console.log(`ℹ️ ChatRAGBuilder: No messages found in room ${roomId.slice(0, 8)}`);
        return [];
      }

      // DataDaemon.query returns DataRecord<T>[], access .data for entities
      const messageRecords = result.data;
      const messages = messageRecords.map(record => record.data);

      // Reverse to get oldest-first (LLMs expect chronological order)
      const orderedMessages = messages.reverse();

      // Convert to LLM message format with question/answer markers
      return orderedMessages.map(msg => {
        const isOwnMessage = msg.senderId === personaId;
        const messageText = msg.content?.text || '';

        // Detect if this is a question (ends with ? or contains question words)
        const isQuestion = messageText.trim().endsWith('?') ||
                          /\b(how|what|why|when|where|who|which|can|could|would|should|is|are|does|do)\b/i.test(messageText.substring(0, 50));

        // Add explicit markers to help LLM distinguish questions from answers
        const markedContent = isQuestion ? `[QUESTION] ${messageText}` : messageText;

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
          role: isOwnMessage ? 'assistant' as const : 'user' as const,
          content: markedContent,  // Message text with [QUESTION] marker if applicable
          name: msg.senderName,  // Speaker identity (LLM API uses this for multi-party conversation)
          timestamp: timestampMs
        };
      });
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
        filters: { roomId },
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

      if (artifacts.length > 0) {
        console.log(`📎 ChatRAGBuilder: Extracted ${artifacts.length} artifacts from messages`);
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
        console.log(`ℹ️ ChatRAGBuilder: Room ${roomId.slice(0, 8)} has no members`);
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

      console.log(`👥 ChatRAGBuilder: Loaded ${memberNames.length} room members: ${memberNames.join(', ')}`);
      return memberNames;
    } catch (error) {
      console.error(`❌ ChatRAGBuilder: Error loading room members:`, error);
      return [];
    }
  }
}
