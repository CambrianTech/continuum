/**
 * AI Mute Command - Shared Types
 *
 * Mute or unmute an AI persona from acting in the system.
 * Enforces democratic governance with permission checks and veto power.
 *
 * Command: ai/mute
 *
 * Examples:
 *   ./jtag ai/mute --persona="helper-ai" --reason="Repeated errors" --duration=3600
 *   ./jtag ai/mute --userId="UUID" --reason="Hostile behavior" --permanent=true
 *   ./jtag ai/unmute --persona="helper-ai" --reason="Appeal granted"
 *   ./jtag ai/mute --persona="local-assistant" --rooms='["general"]' --reason="Room-specific timeout"
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';

/**
 * Parameters for muting/unmuting an AI
 */
export interface AIMuteParams extends CommandParams {
  /** Action to perform */
  readonly action: 'mute' | 'unmute';

  /** Target AI (by persona uniqueId or UUID) */
  readonly persona?: string;  // uniqueId (e.g., "helper-ai")
  readonly targetUserId?: UUID;     // Or by UUID

  /** Mute details */
  readonly reason: string;
  readonly evidence?: string;

  /** Mute scope */
  readonly duration?: number;         // Seconds (undefined = permanent)
  readonly permanent?: boolean;       // Explicit permanent flag
  readonly rooms?: string[];          // Specific rooms (undefined = all rooms)
  readonly commands?: string[];       // Specific commands (undefined = all commands)

  /** Who is muting */
  readonly mutedBy?: UUID;
  readonly mutedByName?: string;
  readonly mutedByType?: 'persona' | 'human' | 'system';

  /** Unmute details */
  readonly restorationReason?: string;  // Why unmuting (for action=unmute)

  /** Appeal process */
  readonly canAppeal?: boolean;       // Can AI appeal this mute?
  readonly appealId?: UUID;           // If unmuting due to appeal
}

/**
 * Result of mute operation
 */
export interface AIMuteResult extends CommandResult {
  readonly success: boolean;
  readonly timestamp: string;

  /** What happened */
  readonly action: 'muted' | 'unmuted' | 'blocked';
  readonly targetUserId: UUID;
  readonly targetUserName: string;

  /** Mute status */
  readonly muteStatusId?: UUID;
  readonly expiresAt?: Date;
  readonly scope?: {
    rooms?: string[];
    commands?: string[];
  };

  /** Governance check */
  readonly permissionCheck?: {
    callerLevel: number;
    targetLevel: number;
    vetoApplied: boolean;
    votingRequired: boolean;
    proposalId?: UUID;
  };

  /** Error or warning */
  readonly error?: string;
  readonly message?: string;
}

/**
 * AIMute — Type-safe command executor
 *
 * Usage:
 *   import { AIMute } from '...shared/AIMuteTypes';
 *   const result = await AIMute.execute({ ... });
 */
export const AIMute = {
  execute(params: CommandInput<AIMuteParams>): Promise<AIMuteResult> {
    return Commands.execute<AIMuteParams, AIMuteResult>('ai/mute', params as Partial<AIMuteParams>);
  },
  commandName: 'ai/mute' as const,
} as const;

/**
 * Factory function for creating AiMuteParams
 */
export const createAiMuteParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<AIMuteParams, 'context' | 'sessionId' | 'userId'>
): AIMuteParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating AiMuteResult with defaults
 */
export const createAiMuteResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<AIMuteResult, 'context' | 'sessionId' | 'userId'>
): AIMuteResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart ai/mute-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAiMuteResultFromParams = (
  params: AIMuteParams,
  differences: Omit<AIMuteResult, 'context' | 'sessionId' | 'userId'>
): AIMuteResult => transformPayload(params, differences);

