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
