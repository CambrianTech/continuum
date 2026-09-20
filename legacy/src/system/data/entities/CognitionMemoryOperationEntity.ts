/**
 * CognitionMemoryOperationEntity - Logs working memory operations
 *
 * Enables observability into:
 * - Memory additions (new thoughts/facts)
 * - Memory removals (intentional forgetting)
 * - Memory evictions (LRU cache management)
 * - Memory access patterns
 *
 * Used for:
 * - Understanding memory pressure
 * - Debugging memory leaks
 * - Optimizing cache sizes
 * - Training data for importance scoring
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { TextField, NumberField, EnumField, CompositeIndex } from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';
import { COLLECTIONS } from '../../shared/Constants';

/**
 * Memory operation type
 */
export type MemoryOperationType = 'add' | 'remove' | 'evict';

/**
 * CognitionMemoryOperationEntity - Complete record of memory operations
 *
 * Composite indexes optimize common observability queries:
 * 1. Recent operations by persona: WHERE personaId = ? ORDER BY sequenceNumber DESC
 * 2. Operations by type: WHERE personaId = ? AND operation = ? ORDER BY sequenceNumber DESC
 * 3. Operations for plan: WHERE planId = ? ORDER BY sequenceNumber DESC
 */
@CompositeIndex({
  name: 'idx_cognition_memory_ops_persona_sequence',
  fields: ['personaId', 'sequenceNumber'],
  direction: 'DESC'
})
@CompositeIndex({
  name: 'idx_cognition_memory_ops_persona_operation',
  fields: ['personaId', 'operation', 'sequenceNumber'],
  direction: 'DESC'
})
@CompositeIndex({
  name: 'idx_cognition_memory_ops_plan',
  fields: ['planId', 'sequenceNumber'],
  direction: 'DESC'
})
export class CognitionMemoryOperationEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.COGNITION_MEMORY_OPERATIONS;

  @TextField({ index: true })
  personaId!: UUID;

  @TextField()
  personaName!: string;

  @TextField({ index: true, nullable: true })
  planId?: UUID;

  @EnumField({ index: true })
  operation!: MemoryOperationType;

  @TextField({ index: true })
  memoryId!: UUID;

  @TextField()
  thoughtType!: string;

  @TextField()
  thoughtContent!: string;

  @NumberField()
  importance!: number;

  @TextField()
  reason!: string;

  @TextField()
  domain!: string;

  @TextField()
  contextId!: UUID;

  @NumberField()
  sequenceNumber!: number;

  constructor() {
    super();

    // Default values
    this.personaId = '' as UUID;
    this.personaName = '';
    this.operation = 'add';
    this.memoryId = '' as UUID;
    this.thoughtType = '';
    this.thoughtContent = '';
    this.importance = 0.5;
    this.reason = '';
    this.domain = '';
    this.contextId = '' as UUID;
    this.sequenceNumber = 0;
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return CognitionMemoryOperationEntity.collection;
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
   * Implement BaseEntity abstract method - validate memory operation data
   */
  validate(): { success: boolean; error?: string } {
    if (!this.personaId?.trim()) {
      return { success: false, error: 'CognitionMemoryOperation personaId is required' };
    }

    if (!this.personaName?.trim()) {
      return { success: false, error: 'CognitionMemoryOperation personaName is required' };
    }

    const validOperations: MemoryOperationType[] = ['add', 'remove', 'evict'];
    if (!validOperations.includes(this.operation)) {
      return { success: false, error: `CognitionMemoryOperation operation must be one of: ${validOperations.join(', ')}` };
    }

    if (!this.memoryId?.trim()) {
      return { success: false, error: 'CognitionMemoryOperation memoryId is required' };
    }

    if (!this.thoughtType?.trim()) {
      return { success: false, error: 'CognitionMemoryOperation thoughtType is required' };
    }

    if (!this.thoughtContent?.trim()) {
      return { success: false, error: 'CognitionMemoryOperation thoughtContent is required' };
    }

    if (typeof this.importance !== 'number' || this.importance < 0 || this.importance > 1) {
      return { success: false, error: 'CognitionMemoryOperation importance must be a number between 0 and 1' };
    }

    if (!this.reason?.trim()) {
      return { success: false, error: 'CognitionMemoryOperation reason is required' };
    }

    if (!this.domain?.trim()) {
      return { success: false, error: 'CognitionMemoryOperation domain is required' };
    }

    if (!this.contextId?.trim()) {
      return { success: false, error: 'CognitionMemoryOperation contextId is required' };
    }

    if (typeof this.sequenceNumber !== 'number' || this.sequenceNumber < 0) {
      return { success: false, error: 'CognitionMemoryOperation sequenceNumber must be a non-negative number' };
    }

    return { success: true };
  }
}
