/**
 * AI Sleep Command - Shared Types
 *
 * Voluntary attention management for AI personas.
 * Allows AIs to put themselves into different awareness states.
 *
 * Modes:
 * - active: Normal operation, respond to everything
 * - mentioned_only: Only respond when directly @mentioned
 * - human_only: Only respond when a human speaks
 * - sleeping: Completely silent until explicitly woken
 * - until_topic: Silent until a new topic is detected
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { JTAGError } from '../../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Valid sleep modes
 */
export type SleepMode = 'active' | 'mentioned_only' | 'human_only' | 'sleeping' | 'until_topic';

/**
 * AI Sleep Command Parameters
 */
export interface AiSleepParams extends CommandParams {
  /** Sleep mode to enter */
  mode: SleepMode;
  /** Optional reason for entering this state (logged for debugging) */
  reason?: string;
  /** Auto-wake after this many minutes (0 or undefined = indefinite) */
  durationMinutes?: number;
  /** Persona to put to sleep (defaults to caller's persona) */
  personaId?: string;
}

/**
 * Factory function for creating AiSleepParams
 */
export const createAiSleepParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    mode: SleepMode;
    reason?: string;
    durationMinutes?: number;
    personaId?: string;
  }
): AiSleepParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  reason: data.reason ?? '',
  durationMinutes: data.durationMinutes ?? 0,
  personaId: data.personaId ?? '',
  ...data
});

/**
 * AI Sleep Command Result
 */
export interface AiSleepResult extends CommandResult {
  success: boolean;
  /** Previous sleep mode */
  previousMode: SleepMode;
  /** New sleep mode */
  newMode: SleepMode;
  /** When the persona will auto-wake (ISO timestamp, null if indefinite) */
  wakesAt: string | null;
  /** Whether the state change was acknowledged */
  acknowledged: boolean;
  /** The persona that was affected */
  personaId: string;
  /** Human-readable explanation */
  message: string;
  error?: JTAGError;
}

/**
 * Factory function for creating AiSleepResult with defaults
 */
export const createAiSleepResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    previousMode?: SleepMode;
    newMode?: SleepMode;
    wakesAt?: string | null;
    acknowledged?: boolean;
    personaId?: string;
    message?: string;
    error?: JTAGError;
  }
): AiSleepResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  previousMode: data.previousMode ?? 'active',
  newMode: data.newMode ?? 'active',
  wakesAt: data.wakesAt ?? null,
  acknowledged: data.acknowledged ?? false,
  personaId: data.personaId ?? '',
  message: data.message ?? '',
  ...data
});

/**
 * Smart AiSleep-specific inheritance from params
 */
export const createAiSleepResultFromParams = (
  params: AiSleepParams,
  differences: Omit<AiSleepResult, 'context' | 'sessionId'>
): AiSleepResult => transformPayload(params, differences);

/**
 * AiSleep — Type-safe command executor
 *
 * Usage:
 *   import { AiSleep } from '...shared/AiSleepTypes';
 *   const result = await AiSleep.execute({ ... });
 */
export const AiSleep = {
  execute(params: CommandInput<AiSleepParams>): Promise<AiSleepResult> {
    return Commands.execute<AiSleepParams, AiSleepResult>('ai/sleep', params as Partial<AiSleepParams>);
  },
  commandName: 'ai/sleep' as const,
} as const;
