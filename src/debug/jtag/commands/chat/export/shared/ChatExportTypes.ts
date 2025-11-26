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

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

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
  filter?: Record<string, unknown>;

  /** Entity type to export (default: 'chat_messages' - future: support all activity types) */
  collection?: string;

  /** Include threading information (reply-to, thread IDs) */
  includeThreading?: boolean;

  /**
   * Auto-bookmark mode: Automatically track last seen message per caller+room
   * - First call: Returns last N messages (default limit)
   * - Subsequent calls: Returns only NEW messages since last call
   * - State key: chat:export:bookmark:{sessionId}:{roomId}
   * (default: false)
   */
  autoBookmark?: boolean;
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
