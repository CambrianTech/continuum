// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Send Message Command - Server Implementation (Simplified)
 * 
 * Simplified version for modular architecture - handles basic message sending.
 * Follows screenshot/navigate pattern - minimal, focused, ~50 lines.
 * Uses shared chat patterns: roomId, senderId, standard result format.
 */

import { SendMessageCommand } from '@chatSendMessage/shared/SendMessageCommand';
import { NetworkError } from '@shared/ErrorTypes';
import { type SendMessageParams, type SendMessageResult, createSendMessageResult } from '@chatSendMessage/shared/SendMessageTypes';

export class SendMessageServerCommand extends SendMessageCommand {

  async execute(params: SendMessageParams): Promise<SendMessageResult> {
    console.log(`💬 SERVER: Processing message for room ${params.roomId}`);

    try {
      // Basic validation
      if (!params.roomId || !params.content) {
        throw new Error('Missing required parameters: roomId or content');
      }

      // Generate message ID
      const messageId = `${params.roomId}-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
      
      // Simulate message processing (in real implementation would store in database, notify participants)
      console.log(`📋 SERVER: Message ${messageId} processed for room ${params.roomId}`);
      console.log(`📝 SERVER: Content: ${params.content.substring(0, 100)}${params.content.length > 100 ? '...' : ''}`);
      
      // Return success result
      return createSendMessageResult(params.context, params.sessionId, {
        success: true,
        messageId,
        roomId: params.roomId,
        deliveredAt: new Date().toISOString()
      });

    } catch (error) {
      console.error(`❌ SERVER: Failed to send message:`, error);
      
      return createSendMessageResult(params.context, params.sessionId, {
        success: false,
        messageId: '',
        roomId: params.roomId,
        error: error instanceof Error ? new NetworkError('chat', error.message, { cause: error }) : new NetworkError('chat', 'Unknown error occurred')
      });
    }
  }
}