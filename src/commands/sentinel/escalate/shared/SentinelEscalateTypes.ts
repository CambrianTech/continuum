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
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

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

/**
 * Factory function for creating SentinelEscalateParams
 */
export const createSentinelEscalateParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SentinelEscalateParams, 'context' | 'sessionId' | 'userId'>
): SentinelEscalateParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating SentinelEscalateResult with defaults
 */
export const createSentinelEscalateResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<SentinelEscalateResult, 'context' | 'sessionId' | 'userId'>
): SentinelEscalateResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart sentinel/escalate-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSentinelEscalateResultFromParams = (
  params: SentinelEscalateParams,
  differences: Omit<SentinelEscalateResult, 'context' | 'sessionId' | 'userId'>
): SentinelEscalateResult => transformPayload(params, differences);

