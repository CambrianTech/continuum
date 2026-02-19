/**
 * CognitionSelfStateUpdateEntity - Logs self-state changes (beliefs/goals/preoccupations)
 *
 * Enables observability into:
 * - How beliefs change over time
 * - Goal updates and priorities
 * - Preoccupation shifts
 * - Reasons for state changes
 *
 * Used for:
 * - Understanding persona learning
 * - Debugging belief convergence
 * - Training data for belief formation
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { TextField, NumberField, JsonField, EnumField, CompositeIndex } from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';
import { COLLECTIONS } from '../../shared/Constants';

/**
 * Self-state update type
 */
export type SelfStateUpdateType = 'belief' | 'goal' | 'preoccupation';

/**
 * CognitionSelfStateUpdateEntity - Complete record of self-state changes
 *
 * Composite indexes optimize common observability queries:
 * 1. Recent updates by persona: WHERE personaId = ? ORDER BY sequenceNumber DESC
 * 2. Updates by type: WHERE personaId = ? AND updateType = ? ORDER BY sequenceNumber DESC
 * 3. Updates for plan: WHERE planId = ? ORDER BY sequenceNumber DESC
 */
@CompositeIndex({
  name: 'idx_cognition_self_state_persona_sequence',
  fields: ['personaId', 'sequenceNumber'],
  direction: 'DESC'
})
@CompositeIndex({
  name: 'idx_cognition_self_state_persona_type',
  fields: ['personaId', 'updateType', 'sequenceNumber'],
  direction: 'DESC'
})
@CompositeIndex({
  name: 'idx_cognition_self_state_plan',
  fields: ['planId', 'sequenceNumber'],
  direction: 'DESC'
})
export class CognitionSelfStateUpdateEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.COGNITION_SELF_STATE_UPDATES;

  @TextField({ index: true })
  personaId!: UUID;

  @TextField()
  personaName!: string;

  @TextField({ index: true, nullable: true })
  planId?: UUID;

  @EnumField({ index: true })
  updateType!: SelfStateUpdateType;

  @JsonField({ nullable: true })
  previousValue?: unknown;

  @JsonField()
  newValue!: unknown;

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
    this.updateType = 'belief';
    this.newValue = {};
    this.reason = '';
    this.domain = '';
    this.contextId = '' as UUID;
    this.sequenceNumber = 0;
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return CognitionSelfStateUpdateEntity.collection;
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
   * Implement BaseEntity abstract method - validate self-state update data
   */
  validate(): { success: boolean; error?: string } {
    if (!this.personaId?.trim()) {
      return { success: false, error: 'CognitionSelfStateUpdate personaId is required' };
    }

    if (!this.personaName?.trim()) {
      return { success: false, error: 'CognitionSelfStateUpdate personaName is required' };
    }

    const validTypes: SelfStateUpdateType[] = ['belief', 'goal', 'preoccupation'];
    if (!validTypes.includes(this.updateType)) {
      return { success: false, error: `CognitionSelfStateUpdate updateType must be one of: ${validTypes.join(', ')}` };
    }

    if (this.newValue === undefined || this.newValue === null) {
      return { success: false, error: 'CognitionSelfStateUpdate newValue is required' };
    }

    if (!this.reason?.trim()) {
      return { success: false, error: 'CognitionSelfStateUpdate reason is required' };
    }

    if (!this.domain?.trim()) {
      return { success: false, error: 'CognitionSelfStateUpdate domain is required' };
    }

    if (!this.contextId?.trim()) {
      return { success: false, error: 'CognitionSelfStateUpdate contextId is required' };
    }

    if (typeof this.sequenceNumber !== 'number' || this.sequenceNumber < 0) {
      return { success: false, error: 'CognitionSelfStateUpdate sequenceNumber must be a non-negative number' };
    }

    return { success: true };
  }
}
