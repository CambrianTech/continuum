/**
 * ToolExecutionLogEntity - Logs every tool/command executed by persona
 *
 * Enables observability into:
 * - Which tools personas are using
 * - Tool execution times and success rates
 * - Tool parameters and results
 * - Errors and failures
 *
 * Used for:
 * - Debugging tool usage patterns
 * - Performance analysis
 * - Cost tracking (if tools have costs)
 * - Training data for tool selection
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { TextField, NumberField, JsonField, EnumField } from '../decorators/FieldDecorators';
import { BaseEntity } from './BaseEntity';
import { COLLECTIONS } from '../../shared/Constants';

/**
 * Tool execution status
 */
export type ToolExecutionStatus = 'success' | 'error';

/**
 * ToolExecutionLogEntity - Complete record of tool execution
 */
export class ToolExecutionLogEntity extends BaseEntity {
  static readonly collection = COLLECTIONS.TOOL_EXECUTION_LOGS;

  @TextField({ index: true })
  personaId!: UUID;

  @TextField()
  personaName!: string;

  @TextField({ index: true, nullable: true })
  planId?: UUID;

  @TextField({ index: true })
  toolName!: string;

  @JsonField()
  toolParams!: unknown;

  @EnumField({ index: true })
  executionStatus!: ToolExecutionStatus;

  @JsonField({ nullable: true })
  toolResult?: unknown;

  @TextField({ nullable: true })
  errorMessage?: string;

  @NumberField()
  durationMs!: number;

  @NumberField()
  startedAt!: number;

  @NumberField()
  completedAt!: number;

  @TextField()
  domain!: string;

  @TextField()
  contextId!: UUID;

  @TextField({ nullable: true })
  triggeredBy?: string;

  @NumberField()
  sequenceNumber!: number;

  constructor() {
    super();

    // Default values
    this.personaId = '' as UUID;
    this.personaName = '';
    this.toolName = '';
    this.toolParams = {};
    this.executionStatus = 'success';
    this.durationMs = 0;
    this.startedAt = 0;
    this.completedAt = 0;
    this.domain = '';
    this.contextId = '' as UUID;
    this.sequenceNumber = 0;
  }

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return ToolExecutionLogEntity.collection;
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
   * Implement BaseEntity abstract method - validate tool execution data
   */
  validate(): { success: boolean; error?: string } {
    if (!this.personaId?.trim()) {
      return { success: false, error: 'ToolExecutionLog personaId is required' };
    }

    if (!this.personaName?.trim()) {
      return { success: false, error: 'ToolExecutionLog personaName is required' };
    }

    if (!this.toolName?.trim()) {
      return { success: false, error: 'ToolExecutionLog toolName is required' };
    }

    const validStatuses: ToolExecutionStatus[] = ['success', 'error'];
    if (!validStatuses.includes(this.executionStatus)) {
      return { success: false, error: `ToolExecutionLog executionStatus must be one of: ${validStatuses.join(', ')}` };
    }

    if (typeof this.durationMs !== 'number' || this.durationMs < 0) {
      return { success: false, error: 'ToolExecutionLog durationMs must be a non-negative number' };
    }

    if (typeof this.startedAt !== 'number' || this.startedAt < 0) {
      return { success: false, error: 'ToolExecutionLog startedAt must be a non-negative number' };
    }

    if (typeof this.completedAt !== 'number' || this.completedAt < 0) {
      return { success: false, error: 'ToolExecutionLog completedAt must be a non-negative number' };
    }

    if (!this.domain?.trim()) {
      return { success: false, error: 'ToolExecutionLog domain is required' };
    }

    if (!this.contextId?.trim()) {
      return { success: false, error: 'ToolExecutionLog contextId is required' };
    }

    if (typeof this.sequenceNumber !== 'number' || this.sequenceNumber < 0) {
      return { success: false, error: 'ToolExecutionLog sequenceNumber must be a non-negative number' };
    }

    return { success: true };
  }
}
