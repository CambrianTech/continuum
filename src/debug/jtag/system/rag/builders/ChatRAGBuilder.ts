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

    // 1. Load persona identity
    const identity = await this.loadPersonaIdentity(personaId);

    // 2. Load recent conversation history from database
    const conversationHistory = await this.loadConversationHistory(
      contextId,
      personaId,
      maxMessages
    );

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
  private async loadPersonaIdentity(personaId: UUID): Promise<PersonaIdentity> {
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
        systemPrompt: this.buildSystemPrompt(user),
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
   * Build system prompt from persona UserEntity
   */
  private buildSystemPrompt(user: UserEntity): string {
    const name = user.displayName;
    const bio = user.profile?.bio || '';
    const capabilities = user.capabilities?.autoResponds
      ? 'You respond naturally to conversations.'
      : 'You participate when mentioned or when the conversation is relevant.';

    return `You are ${name}${bio ? `, ${bio}` : ''}. ${capabilities}

This is a multi-party group chat with humans and AI participants. You'll see messages from different speakers.
Respond naturally and conversationally (1-3 sentences unless more detail is needed).
DO NOT prefix your responses with names or labels - just respond as yourself directly.`;
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

      // Convert to LLM message format
      return orderedMessages.map(msg => {
        const isOwnMessage = msg.senderId === personaId;

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
          content: msg.content?.text || '',  // Raw message text only (name field identifies speaker)
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
              base64: attachment.base64 || attachment.data,
              content: attachment.content,
              metadata: {
                messageId: msg.id,
                senderName: msg.senderName,
                timestamp: msg.timestamp,
                filename: attachment.filename,
                mimeType: attachment.mimeType || attachment.type,
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
  private detectArtifactType(attachment: any): RAGArtifact['type'] {
    const mimeType = attachment.mimeType || attachment.type || '';

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
    personaId: UUID,
    roomId: UUID,
    maxMemories: number
  ): Promise<PersonaMemory[]> {
    // TODO: Query persona_memory collection when implemented
    // For now, return empty array
    return [];
  }
}
