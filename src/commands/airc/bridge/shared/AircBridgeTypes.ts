/**
 * Airc Bridge Command - Shared Types
 *
 * Ingest one AIRC message into Continuum. Normal messages become chat; explicit !continuum directives become bounded development and test commands. This is the inbox-side companion to airc/send: it lets AIRC peers drive Continuum validation without shelling through jtag chat/send or chat/export by hand.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { ParsedAircBridgeMessage } from '@system/airc-bridge/shared/AircBridgeProtocol';

/**
 * Airc Bridge Command Parameters
 */
export interface AircBridgeParams extends CommandParams {
  // Raw AIRC message body. Plain text is bridged into Continuum chat; messages beginning with the command prefix are parsed as bridge directives.
  message: string;
  // AIRC sender nick used for attribution in bridged chat text.
  senderNick?: string;
  // AIRC channel name, with or without leading #. Defaults to general.
  channel?: string;
  // Continuum room name to target. Defaults to general; the AIRC channel is preserved separately for attribution and mirroring.
  room?: string;
  // Directive prefix for test and control messages. Defaults to !continuum.
  commandPrefix?: string;
  // Parse and report intent without executing Continuum commands.
  dryRun?: boolean;
  // Send bridge command responses back to AIRC via the airc CLI.
  mirrorResponse?: boolean;
}

/**
 * Factory function for creating AircBridgeParams
 */
export const createAircBridgeParams = (
  context: JTAGContext,
  sessionId: UUID,
  userId: UUID,
  data: {
    // Raw AIRC message body. Plain text is bridged into Continuum chat; messages beginning with the command prefix are parsed as bridge directives.
    message: string;
    // AIRC sender nick used for attribution in bridged chat text.
    senderNick?: string;
    // AIRC channel name, with or without leading #. Defaults to general.
    channel?: string;
    // Continuum room name to target. Defaults to general; the AIRC channel is preserved separately for attribution and mirroring.
    room?: string;
    // Directive prefix for test and control messages. Defaults to !continuum.
    commandPrefix?: string;
    // Parse and report intent without executing Continuum commands.
    dryRun?: boolean;
    // Send bridge command responses back to AIRC via the airc CLI.
    mirrorResponse?: boolean;
  },
): AircBridgeParams => createPayload(context, sessionId, {
  userId,
  senderNick: data.senderNick ?? '',
  channel: data.channel ?? '',
  room: data.room ?? '',
  commandPrefix: data.commandPrefix ?? '',
  dryRun: data.dryRun ?? false,
  mirrorResponse: data.mirrorResponse ?? false,
  ...data,
});

/**
 * Airc Bridge Command Result
 */
export interface AircBridgeResult extends CommandResult {
  success: boolean;
  // True when the bridge executed the parsed action. Dry runs return handled=false.
  handled: boolean;
  // Structured parser output for the incoming AIRC message.
  parsed: ParsedAircBridgeMessage;
  // Short human and AI readable response for the action.
  responseText?: string;
  // True when response mirroring to AIRC was requested and handed off successfully.
  mirrored?: boolean;
  // AIRC mirror failure, surfaced loudly instead of swallowed.
  mirrorError?: string;
  // Underlying Continuum command result for directives such as chat export or activity list.
  commandResult?: unknown;
  error?: JTAGError;
}

/**
 * Factory function for creating AircBridgeResult with defaults
 */
export const createAircBridgeResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // True when the bridge executed the parsed action. Dry runs return handled=false.
    handled: boolean;
    // Structured parser output for the incoming AIRC message.
    parsed: ParsedAircBridgeMessage;
    // Short human and AI readable response for the action.
    responseText?: string;
    // True when response mirroring to AIRC was requested and handed off successfully.
    mirrored?: boolean;
    // AIRC mirror failure, surfaced loudly instead of swallowed.
    mirrorError?: string;
    // Underlying Continuum command result for directives such as chat export or activity list.
    commandResult?: unknown;
    error?: JTAGError;
  }
): AircBridgeResult => createPayload(context, sessionId, {
  responseText: data.responseText ?? '',
  mirrored: data.mirrored ?? false,
  mirrorError: data.mirrorError ?? '',
  commandResult: data.commandResult ?? undefined,
  ...data
});

/**
 * Smart Airc Bridge-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAircBridgeResultFromParams = (
  params: AircBridgeParams,
  differences: Omit<AircBridgeResult, 'context' | 'sessionId' | 'userId'>
): AircBridgeResult => transformPayload(params, differences);

/**
 * Airc Bridge — Type-safe command executor
 *
 * Usage:
 *   import { AircBridge } from '...shared/AircBridgeTypes';
 *   const result = await AircBridge.execute({ ... });
 */
export const AircBridge = {
  execute(params: CommandInput<AircBridgeParams>): Promise<AircBridgeResult> {
    return Commands.execute<AircBridgeParams, AircBridgeResult>('airc/bridge', params as Partial<AircBridgeParams>);
  },
  commandName: 'airc/bridge' as const,
} as const;
