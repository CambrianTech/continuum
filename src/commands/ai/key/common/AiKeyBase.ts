/**
 * Shared AI key command types.
 *
 * The ai/key/* commands stay modular by verb, while shared params keep
 * provider identity, sync intent, and redacted merge metadata consistent.
 */

import type { CommandParams, CommandResult, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export type AiKeySyncMode = boolean | 'trusted-grid';

export interface AiKeyParams extends CommandParams {
  /** Provider config key or provider alias, e.g. OPENAI_API_KEY or openai. */
  provider?: string;
  /** Request sync after local mutation. Remote execution stays routing context. */
  sync?: AiKeySyncMode;
  /** Optional target node ids for explicit sync/diff/apply flows. */
  targetNodes?: string[];
  /** Build a merge plan without writing. */
  dryRun?: boolean;
}

export interface AiKeyResult extends CommandResult {
  success: boolean;
  provider?: string;
  synced?: boolean;
  syncMode?: AiKeySyncMode;
  targetNodes?: string[];
  mergePlanId?: string;
  error?: JTAGError;
}

export const createAiKeyParams = <T extends Partial<AiKeyParams> = Partial<AiKeyParams>>(
  context: JTAGContext,
  sessionId: UUID,
  data: T & { provider?: string }
): AiKeyParams & T => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  provider: data.provider ?? '',
  ...data
} as AiKeyParams & T);

export const createAiKeyResult = <T extends Partial<AiKeyResult> = Partial<AiKeyResult>>(
  context: JTAGContext,
  sessionId: UUID,
  data: T & { success: boolean; provider?: string }
): AiKeyResult & T => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  provider: data.provider ?? '',
  ...data
} as AiKeyResult & T);
