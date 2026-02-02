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

import type { UUID } from '../../../../../core/types/CrossPlatformUUID';
import type { JTAGClient } from '../../../../../core/client/shared/JTAGClient';
import type { ChatMessageEntity } from '../../../../../data/entities/ChatMessageEntity';
import type { ProcessableMessage } from '../../QueueItemTypes';
import { PersonaGenome, type PersonaGenomeConfig } from '../../PersonaGenome';
import { DataDaemon } from '../../../../../../daemons/data-daemon/shared/DataDaemon';
import { PERSONA_RAG_CONTEXTS_COLLECTION } from '../../../../../data/entities/PersonaRAGContextEntity';

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
  private log: (message: string) => void;

  constructor(
    personaId: UUID,
    displayName: string,
    genomeConfig: PersonaGenomeConfig,
    client?: JTAGClient,
    logger?: (message: string) => void
  ) {
    this.personaId = personaId;
    this.displayName = displayName;
    this.client = client;
    this.log = logger || (() => {});

    // Initialize genome (skill adapters) - pass logger
    this.genome = new PersonaGenome(genomeConfig, logger);
  }

  /**
   * RAG Context Storage - Store conversation context for a room
   * Enables persona to maintain context across sessions
   *
   * Uses DataDaemon for persistence in per-persona database
   */
  async storeRAGContext(roomId: UUID, context: PersonaRAGContext): Promise<void> {
    // Create record ID from persona+room for upsert pattern
    const recordId = `rag-${this.personaId}-${roomId}`;

    // Store context as a simple entity with JSON stringified context
    const record = {
      id: recordId,
      personaId: this.personaId,
      roomId,
      contextJson: JSON.stringify(context)  // Store as JSON string
    };

    try {
      // Check if record exists
      const existing = await DataDaemon.read(PERSONA_RAG_CONTEXTS_COLLECTION, recordId);

      if (existing) {
        // Update existing record (DataDaemon handles updatedAt)
        await DataDaemon.update(PERSONA_RAG_CONTEXTS_COLLECTION, recordId, record as any);
      } else {
        // Create new record
        await DataDaemon.store(PERSONA_RAG_CONTEXTS_COLLECTION, record as any);
      }
    } catch (error) {
      this.log(`❌ Failed to store RAG context: ${error}`);
    }
  }

  /**
   * RAG Context Loading - Load conversation context for a room
   * Returns null if no context exists yet
   *
   * Uses DataDaemon for persistence in per-persona database
   */
  async loadRAGContext(roomId: UUID): Promise<PersonaRAGContext | null> {
    const recordId = `rag-${this.personaId}-${roomId}`;

    try {
      const entity = await DataDaemon.read(PERSONA_RAG_CONTEXTS_COLLECTION, recordId);

      if (!entity) {
        return null;
      }

      // Parse the stored JSON context from the entity's contextJson field
      const storedContext = (entity as Record<string, unknown>).contextJson as string | undefined;

      if (typeof storedContext === 'string') {
        return JSON.parse(storedContext) as PersonaRAGContext;
      }

      return null;
    } catch (error) {
      this.log(`❌ Failed to load RAG context: ${error}`);
      return null;
    }
  }

  /**
   * Update RAG Context - Add new message to context and trim if needed
   */
  async updateRAGContext(roomId: UUID, message: ProcessableMessage): Promise<void> {
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
    // ProcessableMessage.timestamp is always a number (ms since epoch)
    const timestampStr = new Date(message.timestamp).toISOString();

    context.messages.push({
      senderId: message.senderId,
      senderName: message.senderName,
      text: message.content?.text || '',
      timestamp: timestampStr
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
    const recordId = `rag-${this.personaId}-${roomId}`;

    try {
      await DataDaemon.remove(PERSONA_RAG_CONTEXTS_COLLECTION, recordId);
      this.log(`🗑️ Cleared memory for room ${roomId}`);
    } catch (error) {
      this.log(`❌ Failed to clear room memory: ${error}`);
    }
  }

  /**
   * Shutdown - cleanup resources
   */
  async shutdown(): Promise<void> {
    await this.genome.shutdown();
  }
}
