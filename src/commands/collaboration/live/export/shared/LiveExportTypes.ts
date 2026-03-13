/**
 * Live Export Command
 * Export recent utterances from the active live voice session.
 * Mirrors collaboration/chat/export but for voice sessions.
 *
 * Usage:
 *   ./jtag collaboration/live/export
 *   ./jtag collaboration/live/export --limit=20
 *   ./jtag collaboration/live/export --output="/tmp/call-transcript.md"
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export interface LiveExportParams extends CommandParams {
  /** Number of utterances to export (default: 20) */
  limit?: number;

  /** Output file path (optional — prints to stdout if not provided) */
  output?: string;

  /** Explicit session ID (auto-discovered if not provided) */
  callSessionId?: string;

  /** Include emotional annotations (default: true) */
  includeEmotions?: boolean;
}

export interface LiveExportResult extends CommandResult {
  success: boolean;
  message: string;

  /** Number of utterances exported */
  utteranceCount: number;

  /** Markdown content (if output not specified) */
  markdown?: string;

  /** Output file path (if output specified) */
  filepath?: string;

  /** Session ID exported from */
  callSessionId: string;
}

/**
 * LiveExport — Type-safe command executor
 *
 * Usage:
 *   import { LiveExport } from '@commands/collaboration/live/export/shared/LiveExportTypes';
 *   const result = await LiveExport.execute({ limit: 20 });
 */
export const LiveExport = {
  execute(params?: CommandInput<LiveExportParams>): Promise<LiveExportResult> {
    return Commands.execute<LiveExportParams, LiveExportResult>('collaboration/live/export', params as Partial<LiveExportParams>);
  },
  commandName: 'collaboration/live/export' as const,
} as const;

/**
 * Factory function for creating CollaborationLiveExportParams
 */
export const createCollaborationLiveExportParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<LiveExportParams, 'context' | 'sessionId' | 'userId'>
): LiveExportParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating CollaborationLiveExportResult with defaults
 */
export const createCollaborationLiveExportResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<LiveExportResult, 'context' | 'sessionId' | 'userId'>
): LiveExportResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart collaboration/live/export-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCollaborationLiveExportResultFromParams = (
  params: LiveExportParams,
  differences: Omit<LiveExportResult, 'context' | 'sessionId' | 'userId'>
): LiveExportResult => transformPayload(params, differences);

