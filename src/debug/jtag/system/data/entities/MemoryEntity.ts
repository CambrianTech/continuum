/**
 * Memory Entity - Decorated MemoryData for Hippocampus episodic memory storage
 *
 * Uses field decorators to define storage requirements for per-persona memory databases.
 * Each PersonaUser has their own longterm.db with a memories table.
 */

import { BaseEntity } from './BaseEntity';
import { TextField, JsonField, NumberField, DateField, EnumField } from '../decorators/FieldDecorators';

/**
 * Memory types - categorize different kinds of memories
 */
export enum MemoryType {
  CHAT = 'chat',
  OBSERVATION = 'observation',
  TASK = 'task',
  DECISION = 'decision',
  TOOL_USE = 'tool-use',
  ERROR = 'error',
  INSIGHT = 'insight'
}

/**
 * MemoryEntity - Episodic memory storage for PersonaUser
 *
 * Stores consolidated memories from working memory into long-term storage.
 * Each persona has dedicated database with proper schema via decorators.
 */
export class MemoryEntity extends BaseEntity {
  static readonly collection = 'memories';

  // Memory ownership
  @TextField({ index: true })
  personaId!: string;

  @TextField({ index: true })
  sessionId!: string;

  // Memory content
  @EnumField({ index: true })
  type!: MemoryType;

  @TextField()
  content!: string;

  @JsonField()
  context!: Record<string, any>;

  // Temporal information
  @DateField({ index: true })
  timestamp!: Date;

  @DateField({ nullable: true })
  consolidatedAt?: Date;

  @DateField({ nullable: true })
  lastAccessedAt?: Date;

  // Importance/relevance
  @NumberField()
  importance!: number;

  @NumberField()
  accessCount!: number;

  // Relationships
  @JsonField()
  relatedTo!: string[];

  @JsonField()
  tags!: string[];

  // Metadata
  @TextField()
  source!: string;

  @JsonField({ nullable: true })
  embedding?: number[];

  // Implement abstract methods from BaseEntity
  get collection(): string {
    return MemoryEntity.collection;
  }

  validate(): { success: boolean; error?: string } {
    if (!this.personaId) {
      return { success: false, error: 'personaId is required' };
    }
    if (!this.sessionId) {
      return { success: false, error: 'sessionId is required' };
    }
    if (!this.type) {
      return { success: false, error: 'type is required' };
    }
    if (!this.content) {
      return { success: false, error: 'content is required' };
    }
    if (!this.timestamp) {
      return { success: false, error: 'timestamp is required' };
    }
    if (this.importance === undefined || this.importance === null) {
      return { success: false, error: 'importance is required' };
    }
    if (this.accessCount === undefined || this.accessCount === null) {
      return { success: false, error: 'accessCount is required' };
    }
    if (!this.source) {
      return { success: false, error: 'source is required' };
    }

    return { success: true };
  }
}
