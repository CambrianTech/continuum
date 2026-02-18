/**
 * Chat Export Command
 * Export chat messages to markdown format
 *
 * Supports flexible filtering - can filter by:
 * - Room (name or ID)
 * - Entity type (to support future universal activity export)
 * - After timestamp/message ID
 * - Custom filter object (passed to data/list)
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export interface ChatExportParams extends CommandParams {
  /** Room name or ID to export from (optional - if not provided, exports all rooms) */
  room?: string;

  /** Start from this message ID (exports all messages after this) */
  afterMessageId?: UUID;

  /** Start from this timestamp (exports all messages after this) */
  afterTimestamp?: Date | string;

  /** Number of messages to export (default: 50) */
  limit?: number;

  /** Output file path (optional - if not provided, prints to stdout) */
  output?: string;

  /** Include system messages (default: false) */
  includeSystem?: boolean;

  /** Include test messages (default: false) */
  includeTests?: boolean;

  /** Custom filter object (passed directly to data/list) */
  filter?: Record<string, any>;

  /** Entity type to export (default: 'chat_messages' - future: support all activity types) */
  collection?: string;

  /** Include threading information (reply-to, thread IDs) */
  includeThreading?: boolean;
}

export interface ChatExportResult extends CommandResult {
  success: boolean;
  message: string;

  /** Number of messages exported */
  messageCount: number;

  /** Markdown content (if output not specified) */
  markdown?: string;

  /** Output file path (if output specified) */
  filepath?: string;

  /** Collection/entity type exported */
  collection: string;
}

/**
 * ChatExport — Type-safe command executor
 *
 * Usage:
 *   import { ChatExport } from '@commands/collaboration/chat/export/shared/ChatExportTypes';
 *   const result = await ChatExport.execute({ room: 'general', limit: 50 });
 */
export const ChatExport = {
  execute(params?: CommandInput<ChatExportParams>): Promise<ChatExportResult> {
    return Commands.execute<ChatExportParams, ChatExportResult>('collaboration/chat/export', params as Partial<ChatExportParams>);
  },
  commandName: 'collaboration/chat/export' as const,
} as const;
