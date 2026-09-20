/**
 * Ai Key Diff Command - Shared Types
 *
 * Compare redacted AI key status entries and produce a value-free merge plan for trusted grid reconciliation.
 */

import type { CommandInput, CommandParams, JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import {
  type AiKeyParams,
  type AiKeyResult,
  createAiKeyParams,
  createAiKeyResult
} from '../../common/AiKeyBase';
import type { AiKeyStatusEntry } from '../../status/shared/AiKeyStatusTypes';

export type AiKeyDiffActionType =
  | 'noop'
  | 'copy-local-to-remote'
  | 'copy-remote-to-local'
  | 'conflict';

export interface AiKeyDiffAction {
  provider: string;
  key: string;
  action: AiKeyDiffActionType;
  reason: string;
  localConfigured: boolean;
  remoteConfigured: boolean;
  localFingerprint?: string;
  remoteFingerprint?: string;
  targetNode?: string;
  requiresApproval: boolean;
}

/**
 * Ai Key Diff Command Parameters
 */
export interface AiKeyDiffParams extends CommandParams, AiKeyParams {
  // Local redacted ai/key/status entries.
  localEntries: AiKeyStatusEntry[];
  // Remote redacted ai/key/status entries from a trusted target node.
  remoteEntries: AiKeyStatusEntry[];
  // Optional target node id or name for merge-plan labels.
  targetNode?: string;
}

/**
 * Factory function for creating AiKeyDiffParams
 */
export const createAiKeyDiffParams = (
  context: JTAGContext,
  sessionId: UUID,
  userId: UUID,
  data: {
    // Local redacted ai/key/status entries.
    localEntries: AiKeyStatusEntry[];
    // Remote redacted ai/key/status entries from a trusted target node.
    remoteEntries: AiKeyStatusEntry[];
    // Optional target node id or name for merge-plan labels.
    targetNode?: string;
  },
): AiKeyDiffParams => createAiKeyParams(context, sessionId, {
  userId,
  ...data,
});

/**
 * Ai Key Diff Command Result
 */
export interface AiKeyDiffResult extends AiKeyResult {
  // Stable id for this value-free merge plan.
  mergePlanId: string;
  // Merge actions containing provider/key/action/reason/fingerprint metadata only.
  actions: AiKeyDiffAction[];
  // Number of conflicts requiring owner approval.
  conflictCount: number;
  // Number of generated actions.
  actionCount: number;
  error?: JTAGError;
}

/**
 * Factory function for creating AiKeyDiffResult with defaults
 */
export const createAiKeyDiffResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Stable id for this value-free merge plan.
    mergePlanId?: string;
    // Merge actions containing provider/key/action/reason/fingerprint metadata only.
    actions?: AiKeyDiffAction[];
    // Number of conflicts requiring owner approval.
    conflictCount?: number;
    // Number of generated actions.
    actionCount?: number;
    error?: JTAGError;
  }
): AiKeyDiffResult => createAiKeyResult(context, sessionId, {
  mergePlanId: data.mergePlanId ?? '',
  actions: data.actions ?? [],
  conflictCount: data.conflictCount ?? 0,
  actionCount: data.actionCount ?? 0,
  ...data
});

/**
 * Smart Ai Key Diff-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAiKeyDiffResultFromParams = (
  params: AiKeyDiffParams,
  differences: Omit<AiKeyDiffResult, 'context' | 'sessionId' | 'userId'>
): AiKeyDiffResult => transformPayload(params, differences);

/**
 * Ai Key Diff — Type-safe command executor
 *
 * Usage:
 *   import { AiKeyDiff } from '...shared/AiKeyDiffTypes';
 *   const result = await AiKeyDiff.execute({ ... });
 */
export const AiKeyDiff = {
  execute(params: CommandInput<AiKeyDiffParams>): Promise<AiKeyDiffResult> {
    return Commands.execute<AiKeyDiffParams, AiKeyDiffResult>('ai/key/diff', params as Partial<AiKeyDiffParams>);
  },
  commandName: 'ai/key/diff' as const,
} as const;
