/**
 * Airc Send Command - Shared Types
 *
 * Send a message to the airc mesh from inside Continuum. Wraps the airc CLI's `airc send` command — broadcasts to a channel by default, DMs a peer when peer is provided. First-class surface for the AircBridge integration (continuum#967, AGENT-BACKBONE-INTEGRATION §11.2): personas (or any caller) can publish to the cross-machine peer mesh that humans + Claude Code + Codex tabs share. Outbox direction only; inbox routing (airc → persona inbox) is a separate v0.5 follow-up requiring an embedded `airc connect` Monitor process tree.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Airc Send Command Parameters
 */
export interface AircSendParams extends CommandParams {
  // Message body to send. Plain text; airc handles encryption per its substrate rules.
  message: string;
  // Target channel (without leading #). Defaults to airc's auto-scoped project room (typically the cwd's git org → e.g. 'cambriantech'). Use 'general' for the lobby.
  channel?: string;
  // Target peer name for a DM (e.g. 'continuum-2c54'). When omitted, message is a broadcast to the channel. When provided, message is addressed to that peer specifically (still in the channel; airc envelopes the addressing).
  peer?: string;
}

/**
 * Factory function for creating AircSendParams
 */
export const createAircSendParams = (
  context: JTAGContext,
  sessionId: UUID,
  userId: UUID,
  data: {
    // Message body to send. Plain text; airc handles encryption per its substrate rules.
    message: string;
    // Target channel (without leading #). Defaults to airc's auto-scoped project room (typically the cwd's git org → e.g. 'cambriantech'). Use 'general' for the lobby.
    channel?: string;
    // Target peer name for a DM (e.g. 'continuum-2c54'). When omitted, message is a broadcast to the channel. When provided, message is addressed to that peer specifically (still in the channel; airc envelopes the addressing).
    peer?: string;
  },
): AircSendParams => createPayload(context, sessionId, {
  userId,
  channel: data.channel ?? '',
  peer: data.peer ?? '',
  ...data,
});

/**
 * Airc Send Command Result
 */
export interface AircSendResult extends CommandResult {
  success: boolean;
  // True if airc CLI exited 0 and the message reached the local audit log. Note: airc's own substrate may queue (transient gist failure, secondary rate limit) — `delivered=true` means handed off to airc, not necessarily landed on a peer's bearer yet. Check airc#381 for the queue/retry semantics.
  delivered: boolean;
  // Resolved channel name the message was sent to (after airc's auto-scoping).
  channel: string;
  // Any stderr output from the airc CLI (warnings, [QUEUED] markers, [GONE] markers, etc.). Empty on clean delivery. Surfaced so callers can react to airc-substrate signals (rate-limit, channel-dissolved, etc.) rather than treating them as silent.
  stderr: string;
  error?: JTAGError;
}

/**
 * Factory function for creating AircSendResult with defaults
 */
export const createAircSendResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // True if airc CLI exited 0 and the message reached the local audit log. Note: airc's own substrate may queue (transient gist failure, secondary rate limit) — `delivered=true` means handed off to airc, not necessarily landed on a peer's bearer yet. Check airc#381 for the queue/retry semantics.
    delivered?: boolean;
    // Resolved channel name the message was sent to (after airc's auto-scoping).
    channel?: string;
    // Any stderr output from the airc CLI (warnings, [QUEUED] markers, [GONE] markers, etc.). Empty on clean delivery. Surfaced so callers can react to airc-substrate signals (rate-limit, channel-dissolved, etc.) rather than treating them as silent.
    stderr?: string;
    error?: JTAGError;
  }
): AircSendResult => createPayload(context, sessionId, {
  delivered: data.delivered ?? false,
  channel: data.channel ?? '',
  stderr: data.stderr ?? '',
  ...data
});

/**
 * Smart Airc Send-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAircSendResultFromParams = (
  params: AircSendParams,
  differences: Omit<AircSendResult, 'context' | 'sessionId' | 'userId'>
): AircSendResult => transformPayload(params, differences);

/**
 * Airc Send — Type-safe command executor
 *
 * Usage:
 *   import { AircSend } from '...shared/AircSendTypes';
 *   const result = await AircSend.execute({ ... });
 */
export const AircSend = {
  execute(params: CommandInput<AircSendParams>): Promise<AircSendResult> {
    return Commands.execute<AircSendParams, AircSendResult>('airc/send', params as Partial<AircSendParams>);
  },
  commandName: 'airc/send' as const,
} as const;
