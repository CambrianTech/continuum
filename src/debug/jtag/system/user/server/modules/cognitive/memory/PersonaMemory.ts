/**
 * PersonaMemory - Knowledge, Context, and Skills
 *
 * "What do I know?"
 *
 * Responsibilities:
 * - Long-term knowledge (RAG context storage/retrieval)
 * - Working memory (recent conversation context)
 * - LoRA genome (skill adapters and paging)
 * - Memory consolidation
 *
 * Target: ~300 lines
 */

import type { UUID } from '../../../../core/types/CrossPlatformUUID';
import type { JTAGClient } from '../../../../core/client/shared/JTAGClient';
import type { ChatMessageEntity } from '../../../../data/entities/ChatMessageEntity';
import { PersonaGenome, type PersonaGenomeConfig } from '../../PersonaGenome';

/**
 * RAG Context Types - Storage structure for persona conversation context
 */
export interface PersonaRAGMessage {
  senderId: UUID;
  senderName: string;
  text: string;
  timestamp: string;
}

export interface PersonaRAGContext {
  roomId: UUID;
  personaId: UUID;
  messages: PersonaRAGMessage[];
  lastUpdated: string;
  tokenCount: number;
}

/**
 * PersonaMemory - Manages knowledge, context, and skills for a PersonaUser
 */
export class PersonaMemory {
  private client: JTAGClient | undefined;
  private personaId: UUID;
  private displayName: string;
  public genome: PersonaGenome;

  constructor(
    personaId: UUID,
    displayName: string,
    genomeConfig: PersonaGenomeConfig,
    client?: JTAGClient
  ) {
    this.personaId = personaId;
    this.displayName = displayName;
    this.client = client;

    // Initialize genome (skill adapters)
    this.genome = new PersonaGenome(genomeConfig);
  }

  /**
   * RAG Context Storage - Store conversation context for a room
   * Enables persona to maintain context across sessions
   *
   * Phase 2: Direct ArtifactsDaemon access (proper implementation pending)
   * For now, store in memory until artifact commands are implemented
   */
  async storeRAGContext(roomId: UUID, context: PersonaRAGContext): Promise<void> {
    if (!this.client) {
      console.warn(`⚠️  PersonaMemory ${this.displayName}: Cannot store RAG context - no client`);
      return;
    }

    // TODO Phase 2: Use artifacts daemon when commands are implemented
    // await this.client.daemons.artifacts.writeJSON(...)
  }

  /**
   * RAG Context Loading - Load conversation context for a room
   * Returns null if no context exists yet
   *
   * Phase 2: Direct ArtifactsDaemon access (proper implementation pending)
   * For now, return null until artifact commands are implemented
   */
  async loadRAGContext(roomId: UUID): Promise<PersonaRAGContext | null> {
    if (!this.client) {
      console.warn(`⚠️  PersonaMemory ${this.displayName}: Cannot load RAG context - no client`);
      return null;
    }

    // TODO Phase 2: Use artifacts daemon when commands are implemented
    // return await this.client.daemons.artifacts.readJSON<PersonaRAGContext>(...)
    return null;
  }

  /**
   * Update RAG Context - Add new message to context and trim if needed
   */
  async updateRAGContext(roomId: UUID, message: ChatMessageEntity): Promise<void> {
    // Load existing context or create new
    let context = await this.loadRAGContext(roomId);
    if (!context) {
      context = {
        roomId,
        personaId: this.personaId,
        messages: [],
        lastUpdated: new Date().toISOString(),
        tokenCount: 0
      };
    }

    // Add new message to context
    context.messages.push({
      senderId: message.senderId,
      senderName: message.senderName,
      text: message.content?.text || '',
      timestamp: typeof message.timestamp === 'string' ? message.timestamp : message.timestamp.toISOString()
    });

    // Keep only last 50 messages (simple context window for now)
    if (context.messages.length > 50) {
      context.messages = context.messages.slice(-50);
    }

    context.lastUpdated = new Date().toISOString();

    // Store updated context
    await this.storeRAGContext(roomId, context);
  }

  /**
   * Get working memory summary - recent conversation context for decision-making
   * Used by PersonaCognition to determine if we should respond
   */
  async getWorkingMemory(roomId: UUID): Promise<PersonaRAGMessage[]> {
    const context = await this.loadRAGContext(roomId);
    if (!context) {
      return [];
    }

    // Return last 10 messages as working memory
    return context.messages.slice(-10);
  }

  /**
   * Clear memory for a room (e.g., when leaving room or on request)
   */
  async clearRoomMemory(roomId: UUID): Promise<void> {
    // For now, just log (actual clearing will happen when artifact storage is implemented)
    console.log(`🗑️  PersonaMemory ${this.displayName}: Clearing memory for room ${roomId}`);
    // TODO: Delete artifact when artifacts daemon is implemented
  }

  /**
   * Shutdown - cleanup resources
   */
  async shutdown(): Promise<void> {
    await this.genome.shutdown();
  }
}
