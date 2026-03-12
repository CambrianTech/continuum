/**
 * Sentinel Escalate — Types
 *
 * Receives lifecycle push from Rust SentinelModule on completion/failure.
 * All tracking data originates in Rust (single source of truth).
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { EscalationRule } from '../../../../system/sentinel/entities/SentinelEntity';
import { Commands } from '@system/core/shared/Commands';
import type { CommandInput } from '@system/core/types/JTAGTypes';

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

/**
 * SentinelEscalate — Type-safe command executor
 *
 * Usage:
 *   import { SentinelEscalate } from '...shared/SentinelEscalateTypes';
 *   const result = await SentinelEscalate.execute({ ... });
 */
export const SentinelEscalate = {
  execute(params: CommandInput<SentinelEscalateParams>): Promise<SentinelEscalateResult> {
    return Commands.execute<SentinelEscalateParams, SentinelEscalateResult>('sentinel/escalate', params as Partial<SentinelEscalateParams>);
  },
  commandName: 'sentinel/escalate' as const,
} as const;
