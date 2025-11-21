/**
 * AdapterDecisionLogEntity - Logs every decision made by decision adapter chain
 *
 * Enables observability into:
 * - Which adapters are making decisions
 * - Decision patterns (RESPOND vs SILENT)
 * - Confidence levels and reasoning
 * - Adapter evaluation times
 *
 * Used for:
 * - Understanding why personas respond or stay silent
 * - Debugging decision-making logic
 * - Analyzing adapter effectiveness
 * - Training decision models
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { TextField, NumberField, JsonField, EnumField } from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';
import { COLLECTIONS } from '../../shared/Constants';

/**
 * Adapter decision types
 */
export type AdapterDecision = 'RESPOND' | 'SILENT' | 'DEFER' | 'PASS';

/**
 * Decision context metadata
 */
export interface DecisionContextMetadata {
  messageText?: string;
  priority?: number;
  cognitiveLoad?: number;
  isMentioned?: boolean;
  senderIsHuman?: boolean;
  recentMessageCount?: number;
  [key: string]: unknown;  // Allow additional context fields
}

/**
 * AdapterDecisionLogEntity - Complete record of adapter decision
 */
export class AdapterDecisionLogEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.ADAPTER_DECISION_LOGS;

  @TextField({ index: true })
  personaId!: UUID;

  @TextField()
  personaName!: string;

  @TextField({ index: true, nullable: true })
  planId?: UUID;

  @TextField({ index: true })
  adapterName!: string;

  @EnumField({ index: true })
  decision!: AdapterDecision;

  @NumberField()
  confidence!: number;

  @TextField()
  reasoning!: string;

  @JsonField()
  decisionContext!: DecisionContextMetadata;

  @NumberField()
  evaluationDurationMs!: number;

  @NumberField()
  timestamp!: number;

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
    this.decision = 'PASS';
    this.confidence = 0;
    this.reasoning = '';
    this.decisionContext = {};
    this.evaluationDurationMs = 0;
    this.timestamp = 0;
    this.domain = '';
    this.contextId = '' as UUID;
    this.sequenceNumber = 0;
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return AdapterDecisionLogEntity.collection;
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
   * Implement BaseEntity abstract method - validate adapter decision data
   */
  validate(): { success: boolean; error?: string } {
    if (!this.personaId?.trim()) {
      return { success: false, error: 'AdapterDecisionLog personaId is required' };
    }

    if (!this.personaName?.trim()) {
      return { success: false, error: 'AdapterDecisionLog personaName is required' };
    }

    if (!this.adapterName?.trim()) {
      return { success: false, error: 'AdapterDecisionLog adapterName is required' };
    }

    const validDecisions: AdapterDecision[] = ['RESPOND', 'SILENT', 'DEFER', 'PASS'];
    if (!validDecisions.includes(this.decision)) {
      return { success: false, error: `AdapterDecisionLog decision must be one of: ${validDecisions.join(', ')}` };
    }

    if (typeof this.confidence !== 'number' || this.confidence < 0 || this.confidence > 1) {
      return { success: false, error: 'AdapterDecisionLog confidence must be a number between 0 and 1' };
    }

    if (!this.reasoning?.trim()) {
      return { success: false, error: 'AdapterDecisionLog reasoning is required' };
    }

    if (typeof this.evaluationDurationMs !== 'number' || this.evaluationDurationMs < 0) {
      return { success: false, error: 'AdapterDecisionLog evaluationDurationMs must be a non-negative number' };
    }

    if (typeof this.timestamp !== 'number' || this.timestamp < 0) {
      return { success: false, error: 'AdapterDecisionLog timestamp must be a non-negative number' };
    }

    if (!this.domain?.trim()) {
      return { success: false, error: 'AdapterDecisionLog domain is required' };
    }

    if (!this.contextId?.trim()) {
      return { success: false, error: 'AdapterDecisionLog contextId is required' };
    }

    if (typeof this.sequenceNumber !== 'number' || this.sequenceNumber < 0) {
      return { success: false, error: 'AdapterDecisionLog sequenceNumber must be a non-negative number' };
    }

    return { success: true };
  }
}
