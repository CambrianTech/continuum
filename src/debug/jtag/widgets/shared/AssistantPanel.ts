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
import { PositronWidgetState, type PositronicContext } from './services/state/PositronWidgetState';

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
  /** Panel title (default: "AI Assistant") */
  title?: string;
  /** Start collapsed */
  startCollapsed?: boolean;
  /** Panel width when expanded */
  width?: string;
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
  private isCollapsed = false;
  private unsubscribe?: () => void;
  private unsubscribePositron?: () => void;

  private actualRoomId: string | null = null;

  // Positronic awareness - what widget the user is viewing
  private currentContext: PositronicContext | null = null;

  constructor(container: HTMLElement, config: AssistantPanelConfig) {
    this.container = container;
    this.config = {
      placeholder: 'Ask the AI assistant...',
      greeting: 'Hi! How can I help you?',
      maxMessages: 20,
      title: 'AI Assistant',
      startCollapsed: false,
      width: '320px',
      ...config
    };
    this.isCollapsed = this.config.startCollapsed || false;
    this.render();
    this.setupEventListeners();
    this.initializeRoom();
    this.subscribeToPositronState();
  }

  /**
   * Subscribe to Positron widget state for context awareness
   * AIs will know what the user is viewing and can provide contextual help
   */
  private subscribeToPositronState(): void {
    this.unsubscribePositron = PositronWidgetState.subscribe((context) => {
      this.currentContext = context;
      this.updateContextIndicator();
      console.log('🧠 AssistantPanel: Context updated', {
        widgetType: context.widget.widgetType,
        section: context.widget.section
      });
    });
  }

  /**
   * Update the context indicator in the header
   */
  private updateContextIndicator(): void {
    const indicator = this.container.querySelector('.context-indicator');
    if (indicator && this.currentContext) {
      const { widget } = this.currentContext;
      indicator.textContent = widget.section
        ? `📍 ${widget.title}`
        : `📍 ${widget.widgetType}`;
      indicator.setAttribute('title', PositronWidgetState.toRAGContext());
    }
  }

  /** Toggle panel collapsed state */
  toggle(): void {
    this.isCollapsed = !this.isCollapsed;
    const panel = this.container.querySelector('.assistant-panel') as HTMLElement;
    if (panel) {
      panel.classList.toggle('collapsed', this.isCollapsed);
    }
    // Update toggle button icon - » means "collapse", « means "expand"
    const toggleBtn = this.container.querySelector('.toggle-btn');
    if (toggleBtn) {
      toggleBtn.textContent = this.isCollapsed ? '«' : '»';
    }
  }

  /** Expand the panel */
  expand(): void {
    if (this.isCollapsed) this.toggle();
  }

  /** Collapse the panel */
  collapse(): void {
    if (!this.isCollapsed) this.toggle();
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
    const width = this.config.width || '320px';

    // Apply critical styles directly to container so it fills its parent
    // This ensures AssistantPanel works in any context (ThemeWidget, SettingsWidget, etc.)
    this.container.style.display = 'flex';
    this.container.style.height = '100%';
    this.container.style.flexShrink = '0';

    this.container.innerHTML = `
      <style>
        .assistant-panel {
          display: flex;
          flex-direction: column;
          height: 100%;
          width: ${width};
          background: rgba(10, 15, 20, 0.98);
          border-left: 1px solid rgba(0, 212, 255, 0.3);
          transition: width 0.3s ease, opacity 0.3s ease;
          overflow: hidden;
        }

        .assistant-panel.collapsed {
          width: 48px;
        }

        .assistant-panel.collapsed .assistant-content,
        .assistant-panel.collapsed .assistant-input-area,
        .assistant-panel.collapsed .assistant-title-text {
          opacity: 0;
          pointer-events: none;
        }

        .assistant-header {
          padding: 12px;
          background: linear-gradient(180deg, rgba(0, 212, 255, 0.15) 0%, rgba(0, 212, 255, 0.05) 100%);
          border-bottom: 1px solid rgba(0, 212, 255, 0.2);
          display: flex;
          align-items: center;
          gap: 10px;
          min-height: 48px;
          flex-shrink: 0;
        }

        .toggle-btn {
          width: 24px;
          height: 24px;
          background: rgba(0, 212, 255, 0.2);
          border: 1px solid rgba(0, 212, 255, 0.3);
          border-radius: 4px;
          color: #00d4ff;
          cursor: pointer;
          display: flex;
          align-items: center;
          justify-content: center;
          font-size: 10px;
          transition: all 0.2s ease;
          flex-shrink: 0;
        }

        .toggle-btn:hover {
          background: rgba(0, 212, 255, 0.3);
          border-color: #00d4ff;
        }

        .context-bar {
          padding: 6px 12px;
          background: rgba(138, 43, 226, 0.15);
          border-bottom: 1px solid rgba(138, 43, 226, 0.3);
          flex-shrink: 0;
        }

        .context-indicator {
          font-size: 11px;
          color: rgba(200, 150, 255, 0.9);
          cursor: help;
          white-space: nowrap;
          overflow: hidden;
          text-overflow: ellipsis;
          display: block;
        }

        .assistant-panel.collapsed .context-bar {
          opacity: 0;
          pointer-events: none;
        }

        .assistant-icon {
          font-size: 18px;
          flex-shrink: 0;
        }

        .assistant-title-text {
          font-size: 13px;
          font-weight: 600;
          color: #00d4ff;
          white-space: nowrap;
          overflow: hidden;
          transition: opacity 0.2s ease;
        }

        .assistant-content {
          flex: 1;
          display: flex;
          flex-direction: column;
          overflow: hidden;
          transition: opacity 0.2s ease;
        }

        .assistant-messages {
          flex: 1;
          overflow-y: auto;
          padding: 12px;
          display: flex;
          flex-direction: column;
          gap: 10px;
        }

        .assistant-messages::-webkit-scrollbar {
          width: 6px;
        }

        .assistant-messages::-webkit-scrollbar-track {
          background: rgba(0, 0, 0, 0.2);
        }

        .assistant-messages::-webkit-scrollbar-thumb {
          background: rgba(0, 212, 255, 0.3);
          border-radius: 3px;
        }

        .assistant-messages::-webkit-scrollbar-thumb:hover {
          background: rgba(0, 212, 255, 0.5);
        }

        .assistant-message {
          padding: 10px 12px;
          border-radius: 8px;
          font-size: 12px;
          line-height: 1.5;
          max-width: 95%;
          word-wrap: break-word;
        }

        .assistant-message.ai {
          background: rgba(0, 212, 255, 0.08);
          border: 1px solid rgba(0, 212, 255, 0.15);
          color: rgba(255, 255, 255, 0.9);
          align-self: flex-start;
          border-radius: 8px 8px 8px 2px;
        }

        .assistant-message.human {
          background: rgba(100, 150, 200, 0.15);
          border: 1px solid rgba(100, 150, 200, 0.25);
          color: rgba(255, 255, 255, 0.9);
          align-self: flex-end;
          border-radius: 8px 8px 2px 8px;
        }

        .assistant-message.system {
          background: rgba(255, 200, 100, 0.1);
          border: 1px solid rgba(255, 200, 100, 0.2);
          color: rgba(255, 200, 100, 0.9);
          align-self: center;
          font-style: italic;
          font-size: 11px;
        }

        .message-author {
          font-size: 10px;
          font-weight: 600;
          color: rgba(0, 212, 255, 0.7);
          margin-bottom: 4px;
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }

        .assistant-input-area {
          padding: 12px;
          border-top: 1px solid rgba(0, 212, 255, 0.2);
          display: flex;
          gap: 8px;
          background: rgba(0, 10, 15, 0.5);
          flex-shrink: 0;
          transition: opacity 0.2s ease;
        }

        .assistant-input {
          flex: 1;
          padding: 10px 12px;
          background: rgba(0, 10, 15, 0.8);
          border: 1px solid rgba(0, 212, 255, 0.25);
          border-radius: 6px;
          color: white;
          font-size: 12px;
          resize: none;
          transition: border-color 0.2s ease;
        }

        .assistant-input:focus {
          outline: none;
          border-color: #00d4ff;
          box-shadow: 0 0 0 2px rgba(0, 212, 255, 0.1);
        }

        .assistant-input::placeholder {
          color: rgba(255, 255, 255, 0.35);
        }

        .assistant-send {
          padding: 10px 14px;
          background: linear-gradient(135deg, #00d4ff, #0088cc);
          border: none;
          border-radius: 6px;
          color: white;
          font-weight: 600;
          font-size: 12px;
          cursor: pointer;
          transition: all 0.2s ease;
          flex-shrink: 0;
        }

        .assistant-send:hover {
          background: linear-gradient(135deg, #00e5ff, #00aadd);
          transform: translateY(-1px);
        }

        .assistant-send:active {
          transform: translateY(0);
        }

        .assistant-send:disabled {
          opacity: 0.5;
          cursor: not-allowed;
          transform: none;
        }

        .loading-indicator {
          display: flex;
          align-items: center;
          gap: 8px;
          padding: 8px 12px;
          color: rgba(0, 212, 255, 0.7);
          font-size: 11px;
        }

        .loading-dots {
          display: flex;
          gap: 3px;
        }

        .loading-dots span {
          width: 5px;
          height: 5px;
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

      <div class="assistant-panel ${this.isCollapsed ? 'collapsed' : ''}">
        <div class="assistant-header">
          <button class="toggle-btn" id="toggle-panel" title="Toggle panel">
            ${this.isCollapsed ? '«' : '»'}
          </button>
          <span class="assistant-icon">🧠</span>
          <span class="assistant-title-text">${this.config.title}</span>
        </div>
        <div class="context-bar">
          <span class="context-indicator" title="AI sees your current context">📍 Initializing...</span>
        </div>
        <div class="assistant-content">
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
      </div>
    `;
  }

  private setupEventListeners(): void {
    const input = this.container.querySelector('#assistant-input') as HTMLInputElement;
    const sendBtn = this.container.querySelector('#assistant-send') as HTMLButtonElement;
    const toggleBtn = this.container.querySelector('#toggle-panel') as HTMLButtonElement;

    sendBtn?.addEventListener('click', () => this.sendMessage());
    input?.addEventListener('keypress', (e) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        this.sendMessage();
      }
    });

    // Toggle panel collapse/expand
    toggleBtn?.addEventListener('click', () => this.toggle());

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
      // Build message with Positron context for AI awareness
      // Context tells AIs what widget/section the user is viewing
      const contextPrefix = this.currentContext
        ? `[Context: User is viewing ${this.currentContext.widget.title || this.currentContext.widget.widgetType}${this.currentContext.widget.section ? ` > ${this.currentContext.widget.section}` : ''}]\n\n`
        : '';

      // Send to room with context prefix (AIs will see what user is viewing)
      await (Commands as any).execute('collaboration/chat/send', {
        room: this.config.roomName,
        message: contextPrefix + message,
        metadata: {
          positronContext: this.currentContext ? {
            widgetType: this.currentContext.widget.widgetType,
            section: this.currentContext.widget.section,
            title: this.currentContext.widget.title
          } : undefined
        }
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
    this.unsubscribePositron?.();
  }
}
