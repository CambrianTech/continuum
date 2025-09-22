/**
 * Chat Message Rendering Utility
 *
 * Extracted from ChatWidget to reduce its complexity.
 * Handles all message DOM creation and rendering logic.
 */

import type { ChatMessageData } from '../../../system/data/domains/ChatMessage';

/**
 * Handles creating DOM elements for chat messages
 */
export class ChatMessageRenderer {
  constructor(private readonly currentUserId: string) {}

  /**
   * Create a single message DOM element
   */
  createMessageElement(message: ChatMessageData): HTMLElement {
    const isCurrentUser = message.senderId === this.currentUserId;
    const alignment = isCurrentUser ? 'right' : 'left';
    const timestamp = new Date(message.timestamp).toLocaleString();
    const content = message.content?.text || '';

    // TEMPORARY FIX: Hardcode current user for alignment testing
    const tempCurrentUserId = 'user-joel-12345';
    const tempIsCurrentUser = message.senderId === tempCurrentUserId;
    const tempAlignment = tempIsCurrentUser ? 'right' : 'left';

    // Debug logging for alignment issues
    console.log(`🎯 ALIGNMENT DEBUG: senderId="${message.senderId}", hardcodedUserId="${tempCurrentUserId}", isCurrentUser=${tempIsCurrentUser}, alignment=${tempAlignment}`);

    // Create elements using DOM methods - no HTML strings
    const messageRow = document.createElement('div');
    messageRow.className = `message-row ${tempAlignment}`;
    messageRow.setAttribute('data-message-id', message.messageId || message.id);

    const messageBubble = document.createElement('div');
    messageBubble.className = `message-bubble ${tempIsCurrentUser ? 'current-user' : 'other-user'}`;

    const messageHeader = document.createElement('div');
    messageHeader.className = 'message-header';

    const timeSpan = document.createElement('span');
    timeSpan.className = 'message-time';
    timeSpan.textContent = timestamp;
    messageHeader.appendChild(timeSpan);

    const messageContentDiv = document.createElement('div');
    messageContentDiv.className = 'message-content';

    const textContent = document.createElement('p');
    textContent.className = 'text-content chat-message-renderer';
    textContent.setAttribute('data-interactive', 'true');
    textContent.setAttribute('tabindex', '0');
    textContent.textContent = content; // Safe text content, no HTML injection

    messageContentDiv.appendChild(textContent);
    messageBubble.appendChild(messageHeader);
    messageBubble.appendChild(messageContentDiv);
    messageRow.appendChild(messageBubble);

    return messageRow;
  }

  /**
   * Render multiple messages to HTML string (for initial template rendering)
   */
  renderMessages(messages: ChatMessageData[]): string {
    const tempContainer = document.createElement('div');
    messages.forEach(msg => {
      tempContainer.appendChild(this.createMessageElement(msg));
    });
    return tempContainer.innerHTML;
  }

  /**
   * Extract cursor (timestamp) from message
   */
  getCursor(message: ChatMessageData): string {
    return message.timestamp;
  }

  /**
   * Compare message cursors for sorting (newest first)
   */
  compareCursors(a: string, b: string): number {
    return new Date(b).getTime() - new Date(a).getTime();
  }
}