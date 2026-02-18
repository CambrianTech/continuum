/**
 * SentinelEntity — Database-persisted sentinel with execution history and persona ownership
 *
 * Every sentinel belongs to a persona (parentPersonaId). Sentinels are the
 * subconscious threads of persona cognition — tendrils that extend a persona's
 * reach without fragmenting its attention. When a sentinel completes or fails,
 * results are reported back to the owning persona's inbox.
 *
 * Sentinels can also be templates (isTemplate=true), allowing personas to
 * share perfected workflows with each other.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import {
  TextField,
  NumberField,
  BooleanField,
  JsonField,
  ForeignKeyField,
  EnumField,
} from '../../data/decorators/FieldDecorators';
import { BaseEntity } from '../../data/entities/BaseEntity';
import type { SentinelDefinition, SentinelExecutionResult } from '../SentinelDefinition';

/**
 * Escalation conditions — when to wake up the owning persona's consciousness
 */
export type EscalationCondition = 'error' | 'timeout' | 'unfamiliar' | 'approval_needed' | 'complete';

/**
 * Escalation action — what to do when the condition triggers
 */
export type EscalationAction = 'pause' | 'notify' | 'abort';

/**
 * Escalation priority — how urgently to alert the persona
 */
export type EscalationPriority = 'low' | 'normal' | 'high' | 'urgent';

/**
 * An escalation rule that fires when a sentinel reaches a certain condition
 */
export interface EscalationRule {
  condition: EscalationCondition;
  action: EscalationAction;
  priority: EscalationPriority;
}

/**
 * Default escalation rules — notify on error/timeout, notify on completion
 */
export const DEFAULT_ESCALATION_RULES: EscalationRule[] = [
  { condition: 'error', action: 'notify', priority: 'high' },
  { condition: 'timeout', action: 'notify', priority: 'normal' },
  { condition: 'complete', action: 'notify', priority: 'low' },
];

/**
 * Valid sentinel statuses for the entity lifecycle
 */
export const VALID_SENTINEL_STATUSES = [
  'saved',      // Definition saved, not yet run
  'running',    // Currently executing
  'completed',  // Finished successfully
  'failed',     // Finished with error
  'paused',     // Paused (waiting for escalation resolution)
  'cancelled',  // Manually stopped
] as const;

export type SentinelStatus = typeof VALID_SENTINEL_STATUSES[number];

export class SentinelEntity extends BaseEntity {
  static readonly collection = 'sentinels';

  /** The sentinel definition (JSON) */
  @JsonField()
  definition: SentinelDefinition;

  /** Execution history (most recent first) */
  @JsonField()
  executions: SentinelExecutionResult[];

  /** Current lifecycle status */
  @EnumField({ index: true })
  status: SentinelStatus;

  /** Owning persona — every sentinel belongs to someone */
  @ForeignKeyField({ references: 'users.id', index: true, nullable: true })
  parentPersonaId?: UUID;

  /** Current Rust-side handle ID (ephemeral, only valid while running) */
  @TextField({ nullable: true })
  activeHandle?: string;

  /** Who created this sentinel (user or persona ID) */
  @TextField({ nullable: true })
  createdBy?: string;

  /** Whether this is a reusable template */
  @BooleanField({ default: false })
  isTemplate: boolean;

  /** If cloned from a template, the source template ID */
  @ForeignKeyField({ references: 'sentinels.id', nullable: true })
  parentId?: UUID;

  /** Tags for organization and search */
  @JsonField({ nullable: true })
  tags?: string[];

  /** Escalation rules — when to notify the owning persona */
  @JsonField({ nullable: true })
  escalationRules?: EscalationRule[];

  /** How many times this sentinel has been executed */
  @NumberField()
  executionCount: number;

  /** Last execution success/failure */
  @BooleanField({ nullable: true })
  lastSuccess?: boolean;

  /** Last execution timestamp */
  @TextField({ nullable: true })
  lastRunAt?: string;

  // Index signature for compatibility
  [key: string]: unknown;

  constructor() {
    super();
    this.definition = {} as SentinelDefinition;
    this.executions = [];
    this.status = 'saved';
    this.isTemplate = false;
    this.executionCount = 0;
  }

  get collection(): string {
    return SentinelEntity.collection;
  }

  /** Human-readable name from the definition */
  get name(): string {
    return this.definition?.name ?? 'unnamed';
  }

  /** Definition type (build, pipeline, etc.) */
  get type(): string {
    return this.definition?.type ?? 'unknown';
  }

  validate(): { success: boolean; error?: string } {
    if (!this.definition) {
      return { success: false, error: 'definition is required' };
    }

    if (!this.definition.name) {
      return { success: false, error: 'definition.name is required' };
    }

    if (!this.definition.type) {
      return { success: false, error: 'definition.type is required' };
    }

    if (!VALID_SENTINEL_STATUSES.includes(this.status)) {
      return { success: false, error: `status must be one of: ${VALID_SENTINEL_STATUSES.join(', ')}` };
    }

    return { success: true };
  }

  /**
   * Record an execution result and update aggregate fields
   */
  recordExecution(result: SentinelExecutionResult): void {
    // Prepend (most recent first)
    this.executions.unshift(result);

    // Keep execution history bounded (last 50)
    if (this.executions.length > 50) {
      this.executions = this.executions.slice(0, 50);
    }

    this.executionCount++;
    this.lastSuccess = result.success;
    this.lastRunAt = result.startedAt;
    this.updatedAt = new Date();
  }
}
