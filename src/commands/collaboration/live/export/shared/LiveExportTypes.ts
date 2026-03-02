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
