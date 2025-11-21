/**
 * CognitionStateEntity - Captures persona self-awareness state snapshots
 *
 * Enables observability into:
 * - What the persona is currently focused on
 * - Current cognitive load and available capacity
 * - Active preoccupations (things on the persona's mind)
 * - Working memory contents (recent thoughts)
 *
 * Used for:
 * - Real-time monitoring widgets
 * - Debugging persona behavior
 * - Analyzing attention patterns
 * - Understanding cognitive overload
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { TextField, NumberField, JsonField } from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';
import { COLLECTIONS } from '../../shared/Constants';

/**
 * Current focus of the persona
 */
export interface FocusSnapshot {
  primaryActivity: string | null;  // e.g., "chat-response", "task-execution"
  objective: string;                // Human-readable goal
  focusIntensity: number;           // 0.0-1.0
  startedAt: number;                // When focus began
}

/**
 * A preoccupation (something on the persona's mind)
 */
export interface Preoccupation {
  concern: string;
  priority: number;     // 0.0-1.0
  domain: string;       // e.g., "chat", "task", "self"
  createdAt: number;
}

/**
 * Working memory entry (recent thought)
 */
export interface WorkingMemorySnapshot {
  id: UUID;
  domain: string;
  contextId: UUID;
  thoughtType: string;      // e.g., "observation", "reflection", "plan"
  thoughtContent: string;
  importance: number;       // 0.0-1.0
  createdAt: number;
  lastAccessedAt: number;
}

/**
 * Working memory capacity metrics
 */
export interface WorkingMemoryCapacity {
  used: number;
  max: number;
  byDomain: Record<string, number>;
}

/**
 * CognitionStateEntity - Complete snapshot of persona cognition state
 */
export class CognitionStateEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.COGNITION_STATE_SNAPSHOTS;

  @TextField({ index: true })
  personaId!: UUID;

  @TextField()
  personaName!: string;

  @JsonField()
  currentFocus!: FocusSnapshot;

  @NumberField()
  cognitiveLoad!: number;

  @NumberField()
  availableCapacity!: number;

  @JsonField()
  activePreoccupations!: Preoccupation[];

  @JsonField()
  workingMemory!: WorkingMemorySnapshot[];

  @JsonField()
  workingMemoryCapacity!: WorkingMemoryCapacity;

  @TextField()
  domain!: string;

  @TextField({ nullable: true })
  contextId?: UUID;

  @TextField({ nullable: true })
  triggerEvent?: string;

  @NumberField()
  sequenceNumber!: number;

  constructor() {
    super();

    // Default values
    this.personaId = '' as UUID;
    this.personaName = '';
    this.currentFocus = {
      primaryActivity: null,
      objective: '',
      focusIntensity: 0,
      startedAt: 0
    };
    this.cognitiveLoad = 0;
    this.availableCapacity = 1.0;
    this.activePreoccupations = [];
    this.workingMemory = [];
    this.workingMemoryCapacity = { used: 0, max: 10, byDomain: {} };
    this.domain = '';
    this.sequenceNumber = 0;
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return CognitionStateEntity.collection;
  }

  /**
   * Override pagination config - sort by sequence number DESC (newest first)
   */
  static override getPaginationConfig(): {
    defaultSortField: string;
    defaultSortDirection: 'asc' | 'desc';
    defaultPageSize: number;
    cursorField: string;
  } {
    return {
      defaultSortField: 'sequenceNumber',
      defaultSortDirection: 'desc' as const,
      defaultPageSize: 50,
      cursorField: 'sequenceNumber'
    };
  }

  /**
   * Implement BaseEntity abstract method - validate state snapshot data
   */
  validate(): { success: boolean; error?: string } {
    if (!this.personaId?.trim()) {
      return { success: false, error: 'CognitionState personaId is required' };
    }

    if (!this.personaName?.trim()) {
      return { success: false, error: 'CognitionState personaName is required' };
    }

    if (!this.currentFocus || typeof this.currentFocus.objective !== 'string') {
      return { success: false, error: 'CognitionState currentFocus is required' };
    }

    if (typeof this.cognitiveLoad !== 'number' || this.cognitiveLoad < 0 || this.cognitiveLoad > 1) {
      return { success: false, error: 'CognitionState cognitiveLoad must be a number between 0 and 1' };
    }

    if (typeof this.availableCapacity !== 'number' || this.availableCapacity < 0 || this.availableCapacity > 1) {
      return { success: false, error: 'CognitionState availableCapacity must be a number between 0 and 1' };
    }

    if (!Array.isArray(this.activePreoccupations)) {
      return { success: false, error: 'CognitionState activePreoccupations must be an array' };
    }

    if (!Array.isArray(this.workingMemory)) {
      return { success: false, error: 'CognitionState workingMemory must be an array' };
    }

    if (!this.workingMemoryCapacity || typeof this.workingMemoryCapacity.used !== 'number' || typeof this.workingMemoryCapacity.max !== 'number') {
      return { success: false, error: 'CognitionState workingMemoryCapacity must have used and max numbers' };
    }

    if (!this.domain?.trim()) {
      return { success: false, error: 'CognitionState domain is required' };
    }

    if (typeof this.sequenceNumber !== 'number' || this.sequenceNumber < 0) {
      return { success: false, error: 'CognitionState sequenceNumber must be a non-negative number' };
    }

    return { success: true };
  }
}
