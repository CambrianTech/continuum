/**
 * RightPanelWidget - Contextual right panel for assistant, logs, tools
 *
 * Shows AI assistant chat that's contextual to the current view.
 * Can be collapsed/expanded like the left sidebar.
 * Future: Add tabs for switching between Assistant/Logs/Tools
 */

import { BaseWidget } from '../shared/BaseWidget';
import { Events } from '../../system/core/shared/Events';
import { Commands } from '../../system/core/shared/Commands';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import { getDataEventName } from '../../system/core/shared/EventConstants';

interface ChatMessage {
  id: string;
  content: string;
  authorName: string;
  authorType: 'human' | 'ai' | 'system';
  timestamp: Date;
}

// Content type to assistant room mapping
const CONTENT_ROOM_MAP: Record<string, string> = {
  'settings': 'help',
  'theme': 'theme',
  'help': 'help',
  'persona': 'help',
  'diagnostics': 'help',
  'chat': 'help',  // Default to help for chat views
  'browser': 'help'
};

export class RightPanelWidget extends BaseWidget {
  private currentRoom: string = 'help';
  private actualRoomId: string | null = null;
  private messages: ChatMessage[] = [];
  private isCollapsed = false;
  private unsubscribeMessages?: () => void;

  constructor() {
    super({
      widgetName: 'RightPanelWidget',
      template: undefined,
      styles: undefined,
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: true,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('📋 RightPanelWidget: Initializing...');

    // Listen for content changes to update assistant room
    Events.subscribe('content:opened', (data: any) => {
      this.handleContentChange(data?.contentType);
    });

    Events.subscribe('content:switched', (data: any) => {
      this.handleContentChange(data?.contentType);
    });

    // Load initial room
    await this.initializeRoom();

    console.log('✅ RightPanelWidget: Initialized');
  }

  private handleContentChange(contentType?: string): void {
    if (!contentType) return;

    const newRoom = CONTENT_ROOM_MAP[contentType] || 'help';
    if (newRoom !== this.currentRoom) {
      console.log(`📋 RightPanelWidget: Switching room ${this.currentRoom} -> ${newRoom}`);
      this.currentRoom = newRoom;
      this.initializeRoom();
    }
  }

  private async initializeRoom(): Promise<void> {
    // Unsubscribe from previous room
    if (this.unsubscribeMessages) {
      this.unsubscribeMessages();
    }

    try {
      const result = await Commands.execute('data/list', {
        collection: 'rooms',
        filter: { uniqueId: this.currentRoom },
        limit: 1
      } as any) as any;

      if (result?.success && result.items?.[0]) {
        this.actualRoomId = result.items[0].id;
        await this.loadMessages();
        this.subscribeToMessages();
      }
    } catch (error) {
      console.error('RightPanelWidget: Failed to initialize room:', error);
    }

    this.renderWidget();
  }

  private async loadMessages(): Promise<void> {
    if (!this.actualRoomId) return;

    try {
      const result = await Commands.execute('data/list', {
        collection: 'chat_messages',
        filter: { roomId: this.actualRoomId },
        orderBy: [{ field: 'timestamp', direction: 'asc' }],
        limit: 30
      } as any) as any;

      if (result?.success && result.items) {
        this.messages = result.items.map((msg: any) => ({
          id: msg.id,
          content: msg.content,
          authorName: msg.authorName || 'Unknown',
          authorType: msg.metadata?.authorType === 'persona' || msg.metadata?.authorType === 'agent' ? 'ai' : 'human',
          timestamp: new Date(msg.timestamp)
        }));
      }
    } catch (error) {
      console.error('RightPanelWidget: Failed to load messages:', error);
    }
  }

  private subscribeToMessages(): void {
    const eventName = getDataEventName('chat_messages', 'created');
    this.unsubscribeMessages = Events.subscribe(eventName, (msg: any) => {
      if (msg.roomId === this.actualRoomId) {
        this.messages.push({
          id: msg.id,
          content: msg.content,
          authorName: msg.authorName || 'Unknown',
          authorType: msg.metadata?.authorType === 'persona' || msg.metadata?.authorType === 'agent' ? 'ai' : 'human',
          timestamp: new Date(msg.timestamp)
        });
        // Keep only last 30 messages
        if (this.messages.length > 30) {
          this.messages = this.messages.slice(-30);
        }
        this.renderWidget();
        this.scrollToBottom();
      }
    });
  }

  private scrollToBottom(): void {
    const messagesContainer = this.shadowRoot?.querySelector('.messages-container');
    if (messagesContainer) {
      messagesContainer.scrollTop = messagesContainer.scrollHeight;
    }
  }

  protected async renderWidget(): Promise<void> {
    const styles = `
      :host {
        display: flex;
        flex-direction: column;
        height: 100%;
        width: 100%;
        background: var(--sidebar-background, rgba(20, 25, 35, 0.95));
        color: var(--content-primary, #e0e6ed);
      }

      .panel-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 12px 16px;
        border-bottom: 1px solid var(--border-subtle, rgba(255,255,255,0.1));
        background: rgba(0, 0, 0, 0.2);
      }

      .panel-title {
        display: flex;
        align-items: center;
        gap: 8px;
        font-size: 14px;
        font-weight: 600;
        color: var(--content-accent, #00d4ff);
      }

      .panel-title-icon {
        font-size: 16px;
      }

      .collapse-btn {
        background: none;
        border: none;
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
        padding: 4px 8px;
        font-size: 16px;
        transition: color 0.2s;
      }

      .collapse-btn:hover {
        color: var(--content-accent, #00d4ff);
      }

      .assistant-label {
        font-size: 11px;
        color: var(--content-secondary, #8a92a5);
        text-transform: uppercase;
        letter-spacing: 0.5px;
        padding: 8px 16px;
        background: rgba(0, 0, 0, 0.15);
      }

      .messages-container {
        flex: 1;
        overflow-y: auto;
        padding: 12px;
        display: flex;
        flex-direction: column;
        gap: 8px;
      }

      .message {
        padding: 8px 12px;
        border-radius: 8px;
        font-size: 13px;
        line-height: 1.4;
        max-width: 90%;
      }

      .message.human {
        background: var(--button-primary-background, linear-gradient(135deg, #00d4ff, #0099cc));
        color: #000;
        align-self: flex-end;
        border-bottom-right-radius: 2px;
      }

      .message.ai {
        background: rgba(40, 45, 55, 0.8);
        color: var(--content-primary, #e0e6ed);
        align-self: flex-start;
        border-bottom-left-radius: 2px;
      }

      .message-author {
        font-size: 10px;
        font-weight: 600;
        margin-bottom: 4px;
        opacity: 0.7;
      }

      .message.ai .message-author {
        color: var(--content-accent, #00d4ff);
      }

      .input-container {
        padding: 12px;
        border-top: 1px solid var(--border-subtle, rgba(255,255,255,0.1));
        background: rgba(0, 0, 0, 0.2);
      }

      .input-row {
        display: flex;
        gap: 8px;
      }

      .message-input {
        flex: 1;
        padding: 10px 14px;
        border-radius: 20px;
        border: 1px solid var(--input-border, rgba(255,255,255,0.15));
        background: var(--input-background, rgba(40, 45, 55, 0.8));
        color: var(--input-text, #fff);
        font-size: 13px;
        outline: none;
        transition: border-color 0.2s;
      }

      .message-input:focus {
        border-color: var(--input-border-focus, rgba(0, 212, 255, 0.5));
      }

      .message-input::placeholder {
        color: var(--input-placeholder, #8a92a5);
      }

      .send-btn {
        padding: 10px 16px;
        border-radius: 20px;
        border: none;
        background: var(--button-primary-background, linear-gradient(135deg, #00d4ff, #0099cc));
        color: var(--button-primary-text, #000);
        font-weight: 600;
        cursor: pointer;
        transition: transform 0.1s;
      }

      .send-btn:hover {
        transform: scale(1.02);
      }

      .send-btn:active {
        transform: scale(0.98);
      }

      .empty-state {
        flex: 1;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        color: var(--content-secondary, #8a92a5);
        text-align: center;
        padding: 24px;
      }

      .empty-state-icon {
        font-size: 32px;
        margin-bottom: 12px;
        opacity: 0.5;
      }
    `;

    const messagesHtml = this.messages.length > 0
      ? this.messages.map(msg => `
          <div class="message ${msg.authorType}">
            <div class="message-author">${msg.authorName}</div>
            <div class="message-content">${this.escapeHtml(msg.content)}</div>
          </div>
        `).join('')
      : `
          <div class="empty-state">
            <div class="empty-state-icon">💬</div>
            <div>Ask me about this view</div>
          </div>
        `;

    this.shadowRoot!.innerHTML = `
      <style>${styles}</style>
      <div class="panel-header">
        <div class="panel-title">
          <span class="panel-title-icon">🤖</span>
          <span>AI Assistant</span>
        </div>
        <button class="collapse-btn" title="Collapse panel">»</button>
      </div>
      <div class="assistant-label">ASSISTANT</div>
      <div class="messages-container">
        ${messagesHtml}
      </div>
      <div class="input-container">
        <div class="input-row">
          <input type="text" class="message-input" placeholder="Ask for help..." />
          <button class="send-btn">Send</button>
        </div>
      </div>
    `;

    this.setupInputHandlers();
    this.scrollToBottom();
  }

  private setupInputHandlers(): void {
    const input = this.shadowRoot?.querySelector('.message-input') as HTMLInputElement;
    const sendBtn = this.shadowRoot?.querySelector('.send-btn');
    const collapseBtn = this.shadowRoot?.querySelector('.collapse-btn');

    if (input && sendBtn) {
      const sendMessage = async () => {
        const content = input.value.trim();
        if (!content || !this.actualRoomId) return;

        input.value = '';

        try {
          await Commands.execute('collaboration/chat/send', {
            roomId: this.actualRoomId,
            message: content
          } as any);
        } catch (error) {
          console.error('RightPanelWidget: Failed to send message:', error);
        }
      };

      sendBtn.addEventListener('click', sendMessage);
      input.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' && !e.shiftKey) {
          e.preventDefault();
          sendMessage();
        }
      });
    }

    if (collapseBtn) {
      collapseBtn.addEventListener('click', () => {
        this.toggleCollapse();
      });
    }
  }

  private toggleCollapse(): void {
    this.isCollapsed = !this.isCollapsed;

    // Find desktop container and toggle collapsed class
    const continuumWidget = document.querySelector('continuum-widget') as any;
    if (continuumWidget?.shadowRoot) {
      const desktopContainer = continuumWidget.shadowRoot.querySelector('.desktop-container');
      if (desktopContainer) {
        desktopContainer.classList.toggle('right-panel-collapsed', this.isCollapsed);
      }
    }

    // Update button text
    const collapseBtn = this.shadowRoot?.querySelector('.collapse-btn');
    if (collapseBtn) {
      collapseBtn.textContent = this.isCollapsed ? '«' : '»';
    }
  }

  private escapeHtml(text: string): string {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }

  protected async onWidgetCleanup(): Promise<void> {
    if (this.unsubscribeMessages) {
      this.unsubscribeMessages();
    }
  }
}

// Register the custom element
customElements.define('right-panel-widget', RightPanelWidget);
