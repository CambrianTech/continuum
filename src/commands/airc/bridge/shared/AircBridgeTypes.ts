/**
 * AIRC Bridge Command - Shared Types
 *
 * Ingest one AIRC message into Continuum. Normal messages become chat;
 * explicit !continuum directives become bounded development/test commands.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '@system/core/shared/Commands';
import type { ParsedAircBridgeMessage } from '@system/airc-bridge/shared/AircBridgeProtocol';

export interface AircBridgeParams extends CommandParams {
  /** Raw AIRC message body. Normal text is mirrored to Continuum chat. */
  message: string;

  /** AIRC sender nick, used for attribution in bridged chat text. */
  senderNick?: string;

  /** AIRC channel without or with leading #. Defaults to #general. */
  channel?: string;

  /** Continuum room override. Defaults to the AIRC channel name. */
  room?: string;

  /** Directive prefix for test/control messages. Defaults to !continuum. */
  commandPrefix?: string;

  /** Parse and report intent without executing Continuum commands. */
  dryRun?: boolean;

  /** Send command responses back to AIRC via airc/send. */
  mirrorResponse?: boolean;
}

export interface AircBridgeResult extends CommandResult {
  success: boolean;
  handled: boolean;
  parsed: ParsedAircBridgeMessage;
  responseText?: string;
  mirrored?: boolean;
  commandResult?: unknown;
  error?: string;
}

export const createAircBridgeParams = (
  context: JTAGContext,
  sessionId: UUID,
  userId: UUID,
  data: Omit<AircBridgeParams, 'context' | 'sessionId' | 'userId'>,
): AircBridgeParams => createPayload(context, sessionId, { userId, ...data });

export const createAircBridgeResultFromParams = (
  params: AircBridgeParams,
  differences: Omit<AircBridgeResult, 'context' | 'sessionId' | 'userId'>,
): AircBridgeResult => transformPayload(params, differences);

export const AircBridge = {
  execute(params: CommandInput<AircBridgeParams>): Promise<AircBridgeResult> {
    return Commands.execute<AircBridgeParams, AircBridgeResult>('airc/bridge', params as Partial<AircBridgeParams>);
  },
  commandName: 'airc/bridge' as const,
} as const;
