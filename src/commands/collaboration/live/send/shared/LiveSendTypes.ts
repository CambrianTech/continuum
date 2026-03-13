/**
 * Live Send Command
 * Send a text message into the active live voice session.
 * Auto-discovers the active session — no session ID needed.
 *
 * Usage:
 *   ./jtag collaboration/live/send --message="Hello everyone!"
 *   ./jtag collaboration/live/send --message="How are you?" --speakerName="Joel"
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export interface LiveSendParams extends CommandParams {
  /** Message text to send into the live session */
  message: string;

  /** Override speaker name (defaults to current user's display name) */
  speakerName?: string;

  /** Override speaker ID (defaults to current user ID) */
  speakerId?: string;

  /** Explicit session ID (auto-discovered if not provided) */
  callSessionId?: string;

  /** Confidence score (default: 1.0 — typed text is high confidence) */
  confidence?: number;
}

export interface LiveSendResult extends CommandResult {
  success: boolean;
  message: string;

  /** Session the message was sent to */
  callSessionId: string;

  /** Number of AI responders that received the message */
  responderCount: number;
}

/**
 * LiveSend — Type-safe command executor
 *
 * Usage:
 *   import { LiveSend } from '@commands/collaboration/live/send/shared/LiveSendTypes';
 *   const result = await LiveSend.execute({ message: 'Hello!' });
 */
export const LiveSend = {
  execute(params: CommandInput<LiveSendParams>): Promise<LiveSendResult> {
    return Commands.execute<LiveSendParams, LiveSendResult>('collaboration/live/send', params as Partial<LiveSendParams>);
  },
  commandName: 'collaboration/live/send' as const,
} as const;

/**
 * Factory function for creating CollaborationLiveSendParams
 */
export const createCollaborationLiveSendParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<LiveSendParams, 'context' | 'sessionId' | 'userId'>
): LiveSendParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating CollaborationLiveSendResult with defaults
 */
export const createCollaborationLiveSendResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<LiveSendResult, 'context' | 'sessionId' | 'userId'>
): LiveSendResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart collaboration/live/send-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCollaborationLiveSendResultFromParams = (
  params: LiveSendParams,
  differences: Omit<LiveSendResult, 'context' | 'sessionId' | 'userId'>
): LiveSendResult => transformPayload(params, differences);

