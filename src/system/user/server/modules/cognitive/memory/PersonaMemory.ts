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
import type { ProcessableMessage } from '../../QueueItemTypes';
import { PersonaGenome, type PersonaGenomeConfig } from '../../PersonaGenome';
import { ORM } from '../../../../../../daemons/data-daemon/server/ORM';
import type { DbHandle } from '../../../../../../daemons/data-daemon/server/DatabaseHandleRegistry';
import { COLLECTIONS } from '../../../../../../shared/generated-collection-constants';

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
 *
 * All ORM operations use the persona's dedicated DbHandle (per-persona database).
 * Handle is REQUIRED — passed at construction from PersonaUser.personaDbHandle.
 */
export class PersonaMemory {
  private client: JTAGClient | undefined;
  private personaId: UUID;
  private displayName: string;
  private _dbHandle: DbHandle;
  public genome: PersonaGenome;
  private log: (message: string) => void;

  constructor(
    personaId: UUID,
    displayName: string,
    dbHandle: DbHandle,
    genomeConfig: PersonaGenomeConfig,
    client?: JTAGClient,
    logger?: (message: string) => void
  ) {
    this.personaId = personaId;
    this.displayName = displayName;
    this._dbHandle = dbHandle;
    this.client = client;
    this.log = logger || (() => {});

    // Initialize genome (skill adapters) - pass logger (use own log which has the fallback)
    this.genome = new PersonaGenome(genomeConfig, this.log);
  }

  /**
   * Update the database handle after Hippocampus opens longterm.db.
   * Called by LimbicSystem.setDbHandle() during persona initialization.
   */
  set dbHandle(handle: DbHandle) {
    this.log(`Database handle updated: ${this._dbHandle} → ${handle}`);
    this._dbHandle = handle;
  }

  /** Current database handle */
  get dbHandle(): DbHandle {
    return this._dbHandle;
  }

  /**
   * RAG Context Storage - Store conversation context for a room
   * Enables persona to maintain context across sessions
   *
   * Uses persona's dedicated database via DbHandle
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
      const existing = await ORM.read(COLLECTIONS.PERSONA_RAG_CONTEXTS, recordId, this._dbHandle);

      if (existing) {
        // Update existing record (DataDaemon handles updatedAt)
        await ORM.update(COLLECTIONS.PERSONA_RAG_CONTEXTS, recordId, record as any, true, this._dbHandle);
      } else {
        // Create new record
        await ORM.store(COLLECTIONS.PERSONA_RAG_CONTEXTS, record as any, false, this._dbHandle);
      }
    } catch (error) {
      this.log(`❌ Failed to store RAG context: ${error}`);
    }
  }

  /**
   * RAG Context Loading - Load conversation context for a room
   * Returns null if no context exists yet
   *
   * Uses persona's dedicated database via DbHandle
   */
  async loadRAGContext(roomId: UUID): Promise<PersonaRAGContext | null> {
    const recordId = `rag-${this.personaId}-${roomId}`;

    try {
      const entity = await ORM.read(COLLECTIONS.PERSONA_RAG_CONTEXTS, recordId, this._dbHandle);

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
      await ORM.remove(COLLECTIONS.PERSONA_RAG_CONTEXTS, recordId, false, this._dbHandle);
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
