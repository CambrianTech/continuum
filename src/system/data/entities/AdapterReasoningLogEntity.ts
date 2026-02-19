/**
 * AdapterReasoningLogEntity - Logs adapter internal reasoning steps
 *
 * Enables observability into:
 * - How adapters evaluate decisions
 * - Intermediate calculations and heuristics
 * - Confidence scoring logic
 * - Multi-step reasoning chains
 *
 * Used for:
 * - Debugging adapter decision quality
 * - Understanding false positives/negatives
 * - Tuning adapter parameters
 * - Training data for adapter improvement
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { TextField, NumberField, JsonField } from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';
import { COLLECTIONS } from '../../shared/Constants';

/**
 * AdapterReasoningLogEntity - Complete record of adapter reasoning
 */
export class AdapterReasoningLogEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.ADAPTER_REASONING_LOGS;

  @TextField({ index: true })
  personaId!: UUID;

  @TextField()
  personaName!: string;

  @TextField({ index: true, nullable: true })
  planId?: UUID;

  @TextField({ index: true })
  adapterName!: string;

  @TextField()
  stepDescription!: string;

  @JsonField()
  intermediateResult!: unknown;

  @NumberField()
  confidence!: number;

  @NumberField()
  durationMs!: number;

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
    this.adapterName = '';
    this.stepDescription = '';
    this.intermediateResult = {};
    this.confidence = 0.5;
    this.durationMs = 0;
    this.domain = '';
    this.contextId = '' as UUID;
    this.sequenceNumber = 0;
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return AdapterReasoningLogEntity.collection;
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
   * Implement BaseEntity abstract method - validate adapter reasoning data
   */
  validate(): { success: boolean; error?: string } {
    if (!this.personaId?.trim()) {
      return { success: false, error: 'AdapterReasoningLog personaId is required' };
    }

    if (!this.personaName?.trim()) {
      return { success: false, error: 'AdapterReasoningLog personaName is required' };
    }

    if (!this.adapterName?.trim()) {
      return { success: false, error: 'AdapterReasoningLog adapterName is required' };
    }

    if (!this.stepDescription?.trim()) {
      return { success: false, error: 'AdapterReasoningLog stepDescription is required' };
    }

    if (this.intermediateResult === undefined || this.intermediateResult === null) {
      return { success: false, error: 'AdapterReasoningLog intermediateResult is required' };
    }

    if (typeof this.confidence !== 'number' || this.confidence < 0 || this.confidence > 1) {
      return { success: false, error: 'AdapterReasoningLog confidence must be a number between 0 and 1' };
    }

    if (typeof this.durationMs !== 'number' || this.durationMs < 0) {
      return { success: false, error: 'AdapterReasoningLog durationMs must be a non-negative number' };
    }

    if (!this.domain?.trim()) {
      return { success: false, error: 'AdapterReasoningLog domain is required' };
    }

    if (!this.contextId?.trim()) {
      return { success: false, error: 'AdapterReasoningLog contextId is required' };
    }

    if (typeof this.sequenceNumber !== 'number' || this.sequenceNumber < 0) {
      return { success: false, error: 'AdapterReasoningLog sequenceNumber must be a non-negative number' };
    }

    return { success: true };
  }
}
