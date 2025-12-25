/**
 * AssistantPanel - Embeddable AI assistant chat panel
 *
 * A lightweight chat component for embedding in Settings, Help, and other widgets.
 * Connects to a specific room for contextual AI assistance.
 * Think "Clippy but actually useful" - focused, contextual help.
 */

import { Commands } from '../../system/core/shared/Commands';
import { Events } from '../../system/core/shared/Events';
import { getDataEventName } from '../../system/core/shared/EventConstants';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

export interface AssistantPanelConfig {
  /** Room ID for this assistant's chat backend */
  roomId: UUID;
  /** Room name for display */
  roomName: string;
  /** Placeholder text for input */
  placeholder?: string;
  /** Initial greeting message */
  greeting?: string;
  /** Maximum messages to show */
  maxMessages?: number;
}

interface ChatMessage {
  id: string;
  content: string;
  authorName: string;
  authorType: 'human' | 'ai' | 'system';
  timestamp: Date;
}

export class AssistantPanel {
  private container: HTMLElement;
  private config: AssistantPanelConfig;
  private messages: ChatMessage[] = [];
  private isLoading = false;
  private unsubscribe?: () => void;

  private actualRoomId: string | null = null;

  constructor(container: HTMLElement, config: AssistantPanelConfig) {
    this.container = container;
    this.config = {
      placeholder: 'Ask the AI assistant...',
      greeting: 'Hi! How can I help you?',
      maxMessages: 20,
      ...config
    };
    this.render();
    this.setupEventListeners();
    this.initializeRoom();
  }

  private async initializeRoom(): Promise<void> {
    // Look up actual room ID by uniqueId (roomName)
    try {
      const result = await (Commands as any).execute('data/list', {
        collection: 'rooms',
        filter: { uniqueId: this.config.roomName },
        limit: 1
      });
      if (result?.success && result.items?.[0]) {
        this.actualRoomId = result.items[0].id;
        console.log(`AssistantPanel: Found room '${this.config.roomName}' with id ${this.actualRoomId}`);

        // Subscribe to chat message creation events and filter by room ID
        const eventName = getDataEventName('chat_messages', 'created');
        console.log(`AssistantPanel: Subscribing to ${eventName} for room ${this.actualRoomId}`);

        this.unsubscribe = Events.subscribe(eventName, (eventData: any) => {
          try {
            // Extract entity from command response structure
            const entity = eventData?.data?.data || eventData?.data || eventData;

            // Only process messages for this room
            if (entity.roomId !== this.actualRoomId) {
              return; // Skip messages from other rooms
            }

            // Extract message content (may be {text, media} or direct string)
            const content = typeof entity.content === 'object'
              ? entity.content.text
              : entity.content;

            console.log(`AssistantPanel: New message from ${entity.senderName}: ${content?.substring(0, 50)}...`);

            this.addMessage({
              id: entity.id || Date.now().toString(),
              content: content || '',
              authorName: entity.senderName || entity.authorDisplayName || 'AI',
              authorType: entity.senderType === 'human' ? 'human' : 'ai',
              timestamp: new Date()
            });
          } catch (err) {
            console.error('AssistantPanel: Error processing message event:', err);
          }
        });

        this.loadRecentMessages();
      }
    } catch (err) {
      console.error('AssistantPanel: Failed to find room:', err);
    }
  }

  private render(): void {
    this.container.innerHTML = `
      <style>
        .assistant-panel {
          display: flex;
          flex-direction: column;
          height: 100%;
          background: rgba(10, 15, 20, 0.95);
          border: 1px solid rgba(0, 212, 255, 0.2);
          border-radius: 8px;
          overflow: hidden;
        }

        .assistant-header {
          padding: 12px 16px;
          background: rgba(0, 212, 255, 0.1);
          border-bottom: 1px solid rgba(0, 212, 255, 0.2);
          display: flex;
          align-items: center;
          gap: 8px;
        }

        .assistant-icon {
          font-size: 18px;
        }

        .assistant-title {
          font-size: 14px;
          font-weight: 600;
          color: #00d4ff;
        }

        .assistant-messages {
          flex: 1;
          overflow-y: auto;
          padding: 12px;
          display: flex;
          flex-direction: column;
          gap: 12px;
        }

        .assistant-message {
          padding: 10px 14px;
          border-radius: 8px;
          font-size: 13px;
          line-height: 1.5;
          max-width: 90%;
        }

        .assistant-message.ai {
          background: rgba(0, 212, 255, 0.1);
          border: 1px solid rgba(0, 212, 255, 0.2);
          color: rgba(255, 255, 255, 0.9);
          align-self: flex-start;
        }

        .assistant-message.human {
          background: rgba(100, 150, 200, 0.2);
          border: 1px solid rgba(100, 150, 200, 0.3);
          color: rgba(255, 255, 255, 0.9);
          align-self: flex-end;
        }

        .assistant-message.system {
          background: rgba(255, 200, 100, 0.1);
          border: 1px solid rgba(255, 200, 100, 0.2);
          color: rgba(255, 200, 100, 0.9);
          align-self: center;
          font-style: italic;
        }

        .message-author {
          font-size: 11px;
          font-weight: 600;
          color: rgba(0, 212, 255, 0.7);
          margin-bottom: 4px;
        }

        .assistant-input-area {
          padding: 12px;
          border-top: 1px solid rgba(0, 212, 255, 0.2);
          display: flex;
          gap: 8px;
        }

        .assistant-input {
          flex: 1;
          padding: 10px 14px;
          background: rgba(0, 10, 15, 0.8);
          border: 1px solid rgba(0, 212, 255, 0.3);
          border-radius: 6px;
          color: white;
          font-size: 13px;
          resize: none;
        }

        .assistant-input:focus {
          outline: none;
          border-color: #00d4ff;
        }

        .assistant-input::placeholder {
          color: rgba(255, 255, 255, 0.4);
        }

        .assistant-send {
          padding: 10px 16px;
          background: linear-gradient(135deg, #00d4ff, #0099cc);
          border: none;
          border-radius: 6px;
          color: white;
          font-weight: 500;
          cursor: pointer;
          transition: all 0.2s ease;
        }

        .assistant-send:hover {
          background: linear-gradient(135deg, #00e5ff, #00aadd);
        }

        .assistant-send:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }

        .loading-indicator {
          display: flex;
          align-items: center;
          gap: 8px;
          padding: 8px 14px;
          color: rgba(0, 212, 255, 0.7);
          font-size: 12px;
        }

        .loading-dots {
          display: flex;
          gap: 4px;
        }

        .loading-dots span {
          width: 6px;
          height: 6px;
          border-radius: 50%;
          background: #00d4ff;
          animation: bounce 1.4s infinite ease-in-out;
        }

        .loading-dots span:nth-child(1) { animation-delay: -0.32s; }
        .loading-dots span:nth-child(2) { animation-delay: -0.16s; }

        @keyframes bounce {
          0%, 80%, 100% { transform: scale(0); }
          40% { transform: scale(1); }
        }
      </style>

      <div class="assistant-panel">
        <div class="assistant-header">
          <span class="assistant-icon">🤖</span>
          <span class="assistant-title">AI Assistant</span>
        </div>
        <div class="assistant-messages" id="assistant-messages">
          ${this.config.greeting ? `
            <div class="assistant-message ai">
              <div class="message-author">Assistant</div>
              ${this.config.greeting}
            </div>
          ` : ''}
        </div>
        <div class="assistant-input-area">
          <input
            type="text"
            class="assistant-input"
            id="assistant-input"
            placeholder="${this.config.placeholder}"
          />
          <button class="assistant-send" id="assistant-send">Send</button>
        </div>
      </div>
    `;
  }

  private setupEventListeners(): void {
    const input = this.container.querySelector('#assistant-input') as HTMLInputElement;
    const sendBtn = this.container.querySelector('#assistant-send') as HTMLButtonElement;

    sendBtn?.addEventListener('click', () => this.sendMessage());
    input?.addEventListener('keypress', (e) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        this.sendMessage();
      }
    });

    // Event subscription moved to initializeRoom() after room lookup
  }

  private async loadRecentMessages(): Promise<void> {
    if (!this.actualRoomId) return; // Wait for room lookup

    try {
      // Use actual room ID from database lookup
      const result = await (Commands as any).execute('data/list', {
        collection: 'chat_messages',
        filter: { roomId: this.actualRoomId },
        orderBy: [{ field: 'timestamp', direction: 'desc' }],
        limit: this.config.maxMessages
      });

      if (result?.success && result.items) {
        // Reverse to show oldest first
        const messages = result.items.reverse();
        for (const msg of messages) {
          // Handle content.text structure or direct string
          const content = typeof msg.content === 'object' ? msg.content.text : msg.content;
          this.addMessage({
            id: msg.id,
            content: content || '',
            authorName: msg.senderName || msg.authorDisplayName || msg.authorName || 'Unknown',
            authorType: msg.senderType === 'human' ? 'human' : 'ai',
            timestamp: new Date(msg.timestamp)
          }, false); // Don't scroll for initial load
        }
        this.scrollToBottom();
      }
    } catch (err) {
      console.error('AssistantPanel: Failed to load messages:', err);
    }
  }

  private async sendMessage(): Promise<void> {
    const input = this.container.querySelector('#assistant-input') as HTMLInputElement;
    const message = input?.value.trim();
    if (!message || this.isLoading) return;

    // Clear input
    input.value = '';

    // Add user message
    this.addMessage({
      id: Date.now().toString(),
      content: message,
      authorName: 'You',
      authorType: 'human',
      timestamp: new Date()
    });

    // Show loading
    this.setLoading(true);

    try {
      // Send to room - use any type for lightweight widget context
      await (Commands as any).execute('collaboration/chat/send', {
        room: this.config.roomName,
        message: message
      });
    } catch (err) {
      console.error('AssistantPanel: Failed to send message:', err);
      this.addMessage({
        id: Date.now().toString(),
        content: 'Failed to send message. Please try again.',
        authorName: 'System',
        authorType: 'system',
        timestamp: new Date()
      });
    } finally {
      this.setLoading(false);
    }
  }

  private addMessage(message: ChatMessage, scroll = true): void {
    const messagesEl = this.container.querySelector('#assistant-messages');
    if (!messagesEl) return;

    // Trim old messages
    while (this.messages.length >= (this.config.maxMessages || 20)) {
      this.messages.shift();
      messagesEl.firstElementChild?.remove();
    }

    this.messages.push(message);

    const msgEl = document.createElement('div');
    msgEl.className = `assistant-message ${message.authorType}`;
    msgEl.innerHTML = `
      <div class="message-author">${message.authorName}</div>
      ${this.escapeHtml(message.content)}
    `;
    messagesEl.appendChild(msgEl);

    if (scroll) {
      this.scrollToBottom();
    }
  }

  private setLoading(loading: boolean): void {
    this.isLoading = loading;
    const sendBtn = this.container.querySelector('#assistant-send') as HTMLButtonElement;
    if (sendBtn) {
      sendBtn.disabled = loading;
    }

    const messagesEl = this.container.querySelector('#assistant-messages');
    const existingLoader = messagesEl?.querySelector('.loading-indicator');

    if (loading && !existingLoader && messagesEl) {
      const loader = document.createElement('div');
      loader.className = 'loading-indicator';
      loader.innerHTML = `
        <div class="loading-dots">
          <span></span><span></span><span></span>
        </div>
        AI is thinking...
      `;
      messagesEl.appendChild(loader);
      this.scrollToBottom();
    } else if (!loading && existingLoader) {
      existingLoader.remove();
    }
  }

  private scrollToBottom(): void {
    const messagesEl = this.container.querySelector('#assistant-messages');
    if (messagesEl) {
      messagesEl.scrollTop = messagesEl.scrollHeight;
    }
  }

  private escapeHtml(text: string): string {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }

  /** Clean up subscriptions */
  destroy(): void {
    this.unsubscribe?.();
  }
}
