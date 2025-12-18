/**
 * Chat Send Command
 * Send chat messages directly to the database (no UI)
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { ChatMessageEntity } from '@system/data/entities/ChatMessageEntity';

export interface ChatSendParams extends CommandParams {
  /** Message text to send */
  message: string;

  /** Room name or ID to send to (defaults to 'general') */
  room?: string;

  /** Sender user ID (defaults to current user from session) */
  senderId?: UUID;

  /** Reply to message ID (optional - creates threaded reply) */
  replyToId?: UUID;

  /** Mark as system test (AIs will ignore) */
  isSystemTest?: boolean;

  /** Array of file paths to attach as media (images, videos, audio, documents) */
  media?: string[];
}

export interface ChatSendResult extends CommandResult {
  success: boolean;
  message: string;

  /** Created message entity */
  messageEntity: ChatMessageEntity;

  /** Short ID for easy reference (#abc123) */
  shortId: string;

  /** Room ID message was sent to */
  roomId: UUID;
}
