/**
 * TimelineEventEntity - Global timeline for unified consciousness
 *
 * Records ALL events across ALL contexts for a persona.
 * This is the foundation for cross-context awareness - "no severance".
 *
 * Unlike MemoryEntity (consolidated LTM), TimelineEventEntity captures:
 * - Every message received and sent
 * - Every action taken
 * - Context switches between rooms/activities
 * - Intentions formed and completed
 *
 * This enables:
 * - "What was I doing before this?" queries
 * - Cross-context semantic search
 * - Peripheral awareness of other rooms
 * - Temporal continuity of self
 */

import { BaseEntity } from './BaseEntity';
import { TextField, JsonField, NumberField, DateField, EnumField, CompositeIndex, BlobField } from '../decorators/FieldDecorators';
import type { UUID } from '../../core/types/CrossPlatformUUID';

/**
 * Context types - where the event occurred
 */
export type ContextType = 'room' | 'canvas' | 'browser' | 'direct' | 'self' | 'task';

/**
 * Event types - what kind of event
 */
export type TimelineEventType =
  | 'message_received'     // Someone else sent a message
  | 'message_sent'         // I sent a message
  | 'action_taken'         // I performed an action (tool use, etc.)
  | 'action_observed'      // Someone else performed an action
  | 'context_entered'      // I started focusing on this context
  | 'context_left'         // I stopped focusing on this context
  | 'intention_formed'     // I decided to work on something
  | 'intention_advanced'   // Made progress on a goal
  | 'intention_completed'  // Finished a goal
  | 'observation'          // Noticed something important
  | 'insight'              // Realized something
  | 'learning';            // Learned something new

/**
 * TimelineEventEntity - A single event in the global timeline
 *
 * Composite indexes optimize common queries:
 * 1. Recent events: WHERE personaId = ? ORDER BY timestamp DESC
 * 2. Events by context: WHERE personaId = ? AND contextId = ? ORDER BY timestamp DESC
 * 3. Events by actor: WHERE personaId = ? AND actorId = ? ORDER BY timestamp DESC
 * 4. Events by type: WHERE personaId = ? AND eventType = ? ORDER BY timestamp DESC
 * 5. Cross-context search: Semantic search via embedding
 */
@CompositeIndex({
  name: 'idx_timeline_persona_timestamp',
  fields: ['personaId', 'timestamp'],
  direction: 'DESC'
})
@CompositeIndex({
  name: 'idx_timeline_context_timestamp',
  fields: ['personaId', 'contextId', 'timestamp'],
  direction: 'DESC'
})
@CompositeIndex({
  name: 'idx_timeline_actor_timestamp',
  fields: ['personaId', 'actorId', 'timestamp'],
  direction: 'DESC'
})
@CompositeIndex({
  name: 'idx_timeline_type_timestamp',
  fields: ['personaId', 'eventType', 'timestamp'],
  direction: 'DESC'
})
export class TimelineEventEntity extends BaseEntity {
  static readonly collection = 'timeline_events';

  // Ownership - which persona's timeline
  @TextField({ index: true })
  personaId!: UUID;

  // When
  @DateField({ index: true })
  timestamp!: Date;

  // Context - where it happened
  @EnumField({ index: true })
  contextType!: ContextType;

  @TextField({ index: true })
  contextId!: UUID;

  @TextField()
  contextName!: string;

  // What happened
  @EnumField({ index: true })
  eventType!: TimelineEventType;

  // Who was involved (not necessarily the persona)
  @TextField({ index: true })
  actorId!: UUID;

  @TextField()
  actorName!: string;

  // The actual content
  @TextField()
  content!: string;

  // Importance for filtering/prioritization (0.0 - 1.0)
  @NumberField()
  importance!: number;

  // Topics for semantic linking
  @JsonField()
  topics!: string[];

  // Relationships to other events
  @JsonField({ nullable: true })
  relatedEventIds?: UUID[];

  @TextField({ nullable: true })
  triggeredById?: UUID;

  @JsonField({ nullable: true })
  triggersIds?: UUID[];

  // Embedding for semantic search (cross-context awareness)
  @BlobField({ nullable: true })
  embedding?: number[];

  // Additional metadata
  @JsonField({ nullable: true })
  metadata?: Record<string, unknown>;

  // Implement abstract methods from BaseEntity
  get collection(): string {
    return TimelineEventEntity.collection;
  }

  validate(): { success: boolean; error?: string } {
    if (!this.personaId) {
      return { success: false, error: 'personaId is required' };
    }
    if (!this.timestamp) {
      return { success: false, error: 'timestamp is required' };
    }
    if (!this.contextType) {
      return { success: false, error: 'contextType is required' };
    }
    if (!this.contextId) {
      return { success: false, error: 'contextId is required' };
    }
    if (!this.contextName) {
      return { success: false, error: 'contextName is required' };
    }
    if (!this.eventType) {
      return { success: false, error: 'eventType is required' };
    }
    if (!this.actorId) {
      return { success: false, error: 'actorId is required' };
    }
    if (!this.actorName) {
      return { success: false, error: 'actorName is required' };
    }
    if (!this.content) {
      return { success: false, error: 'content is required' };
    }
    if (this.importance === undefined || this.importance === null) {
      return { success: false, error: 'importance is required' };
    }
    if (!Array.isArray(this.topics)) {
      return { success: false, error: 'topics must be an array' };
    }

    return { success: true };
  }

  /**
   * Static pagination config for timeline queries
   * Most recent first, 50 per page for timeline browsing
   */
  static override getPaginationConfig() {
    return {
      defaultSortField: 'timestamp',
      defaultSortDirection: 'desc' as const,
      defaultPageSize: 50,
      cursorField: 'timestamp'
    };
  }
}

/**
 * Filter for querying timeline events
 */
export interface TimelineFilter {
  personaId: UUID;
  contextId?: UUID;
  contextType?: ContextType;
  eventType?: TimelineEventType | TimelineEventType[];
  actorId?: UUID;
  minImportance?: number;
  since?: Date;
  until?: Date;
  topics?: string[];
  limit?: number;
  excludeContextId?: UUID; // For cross-context queries
}

/**
 * Result of a timeline query with context
 */
export interface TimelineQueryResult {
  events: TimelineEventEntity[];
  totalCount: number;
  oldestTimestamp?: Date;
  newestTimestamp?: Date;
  contextBreakdown?: Record<UUID, number>; // Count by context
}
