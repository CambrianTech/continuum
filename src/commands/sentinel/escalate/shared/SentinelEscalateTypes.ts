/**
 * Sentinel Escalate — Types
 *
 * Receives lifecycle push from Rust SentinelModule on completion/failure.
 * All tracking data originates in Rust (single source of truth).
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { EscalationRule } from '../../../../system/sentinel/entities/SentinelEntity';

/**
 * Pushed from Rust when a sentinel reaches a terminal state.
 */
export interface SentinelEscalateParams extends CommandParams {
  /** Rust-side sentinel handle ID */
  handle: string;
  /** Terminal status */
  status: 'completed' | 'failed' | 'cancelled';
  /** Execution duration in milliseconds */
  durationMs?: number;
  /** Error message if failed */
  error?: string;
  /** Owning persona for inbox delivery */
  parentPersonaId?: string;
  /** SentinelEntity ID for execution history */
  entityId?: string;
  /** Human-readable sentinel name */
  sentinelName: string;
  /** Escalation rules (optional — defaults applied in service) */
  escalationRules?: EscalationRule[];
}

export interface SentinelEscalateResult extends CommandResult {
  /** Whether escalation was processed */
  processed: boolean;
}
