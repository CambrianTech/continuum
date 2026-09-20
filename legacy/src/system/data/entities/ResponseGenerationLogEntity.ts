/**
 * ResponseGenerationLogEntity - Logs AI response generation details
 *
 * Enables observability into:
 * - Which models are being used
 * - Token usage and costs
 * - Response generation times
 * - Prompt and response content (truncated for privacy/size)
 *
 * Used for:
 * - Cost tracking and optimization
 * - Performance analysis
 * - Model comparison
 * - Training data curation
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { TextField, NumberField, EnumField } from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';
import { COLLECTIONS } from '../../shared/Constants';

/**
 * Response generation status
 */
export type ResponseStatus = 'success' | 'error' | 'timeout';

/**
 * ResponseGenerationLogEntity - Complete record of AI response generation
 */
export class ResponseGenerationLogEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.RESPONSE_GENERATION_LOGS;

  @TextField({ index: true })
  personaId!: UUID;

  @TextField()
  personaName!: string;

  @TextField({ index: true, nullable: true })
  planId?: UUID;

  @TextField({ index: true })
  provider!: string;

  @TextField({ index: true })
  model!: string;

  @TextField()
  promptSummary!: string;

  @NumberField()
  promptTokens!: number;

  @NumberField()
  completionTokens!: number;

  @NumberField()
  totalTokens!: number;

  @NumberField()
  estimatedCost!: number;

  @TextField()
  responseSummary!: string;

  @NumberField()
  durationMs!: number;

  @EnumField({ index: true })
  status!: ResponseStatus;

  @TextField({ nullable: true })
  errorMessage?: string;

  @NumberField()
  temperature!: number;

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
    this.provider = '';
    this.model = '';
    this.promptSummary = '';
    this.promptTokens = 0;
    this.completionTokens = 0;
    this.totalTokens = 0;
    this.estimatedCost = 0;
    this.responseSummary = '';
    this.durationMs = 0;
    this.status = 'success';
    this.temperature = 0.7;
    this.timestamp = 0;
    this.domain = '';
    this.contextId = '' as UUID;
    this.sequenceNumber = 0;
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return ResponseGenerationLogEntity.collection;
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
   * Implement BaseEntity abstract method - validate response generation data
   */
  validate(): { success: boolean; error?: string } {
    if (!this.personaId?.trim()) {
      return { success: false, error: 'ResponseGenerationLog personaId is required' };
    }

    if (!this.personaName?.trim()) {
      return { success: false, error: 'ResponseGenerationLog personaName is required' };
    }

    if (!this.provider?.trim()) {
      return { success: false, error: 'ResponseGenerationLog provider is required' };
    }

    if (!this.model?.trim()) {
      return { success: false, error: 'ResponseGenerationLog model is required' };
    }

    if (!this.promptSummary?.trim()) {
      return { success: false, error: 'ResponseGenerationLog promptSummary is required' };
    }

    if (typeof this.promptTokens !== 'number' || this.promptTokens < 0) {
      return { success: false, error: 'ResponseGenerationLog promptTokens must be a non-negative number' };
    }

    if (typeof this.completionTokens !== 'number' || this.completionTokens < 0) {
      return { success: false, error: 'ResponseGenerationLog completionTokens must be a non-negative number' };
    }

    if (typeof this.totalTokens !== 'number' || this.totalTokens < 0) {
      return { success: false, error: 'ResponseGenerationLog totalTokens must be a non-negative number' };
    }

    if (typeof this.estimatedCost !== 'number' || this.estimatedCost < 0) {
      return { success: false, error: 'ResponseGenerationLog estimatedCost must be a non-negative number' };
    }

    if (!this.responseSummary?.trim()) {
      return { success: false, error: 'ResponseGenerationLog responseSummary is required' };
    }

    if (typeof this.durationMs !== 'number' || this.durationMs < 0) {
      return { success: false, error: 'ResponseGenerationLog durationMs must be a non-negative number' };
    }

    const validStatuses: ResponseStatus[] = ['success', 'error', 'timeout'];
    if (!validStatuses.includes(this.status)) {
      return { success: false, error: `ResponseGenerationLog status must be one of: ${validStatuses.join(', ')}` };
    }

    if (typeof this.temperature !== 'number' || this.temperature < 0 || this.temperature > 2) {
      return { success: false, error: 'ResponseGenerationLog temperature must be a number between 0 and 2' };
    }

    if (typeof this.timestamp !== 'number' || this.timestamp < 0) {
      return { success: false, error: 'ResponseGenerationLog timestamp must be a non-negative number' };
    }

    if (!this.domain?.trim()) {
      return { success: false, error: 'ResponseGenerationLog domain is required' };
    }

    if (!this.contextId?.trim()) {
      return { success: false, error: 'ResponseGenerationLog contextId is required' };
    }

    if (typeof this.sequenceNumber !== 'number' || this.sequenceNumber < 0) {
      return { success: false, error: 'ResponseGenerationLog sequenceNumber must be a non-negative number' };
    }

    return { success: true };
  }
}
