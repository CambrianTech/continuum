/**
 * Chat Widget - JTAG-based Web Component
 * 
 * Simple chat interface that uses JTAG command routing for real functionality.
 * Follows the legacy widget pattern but integrates with JTAG system.
 */

import { WidgetBase } from '../shared/WidgetBase';

export class ChatWidget extends WidgetBase {
  private messages: Array<{id: string, content: string, type: 'user' | 'assistant', timestamp: Date}> = [];
  private inputElement: HTMLInputElement | null = null;

  static get widgetName(): string {
    return 'chat';
  }

  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
  }

  connectedCallback() {
    this.render();
    this.setupEventListeners();
  }

  private render() {
    if (!this.shadowRoot) return;

    this.shadowRoot.innerHTML = `
      <style>
        :host {
          display: flex;
          flex-direction: column;
          height: 100%;
          background: #1a1f2e;
          color: #e0e6ed;
          font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
        }
        
        .chat-header {
          padding: 12px 16px;
          background: #2d3748;
          border-bottom: 1px solid #4a5568;
          font-weight: 600;
        }
        
        .messages {
          flex: 1;
          overflow-y: auto;
          padding: 16px;
        }
        
        .message {
          margin-bottom: 12px;
          padding: 8px 12px;
          border-radius: 8px;
        }
        
        .message.user {
          background: #2b6cb0;
          margin-left: 40px;
        }
        
        .message.assistant {
          background: #2d543d;
          margin-right: 40px;
        }
        
        .input-area {
          padding: 16px;
          background: #2d3748;
          border-top: 1px solid #4a5568;
          display: flex;
          gap: 8px;
        }
        
        .message-input {
          flex: 1;
          padding: 8px 12px;
          background: #4a5568;
          border: 1px solid #718096;
          border-radius: 6px;
          color: #e0e6ed;
          font-size: 14px;
        }
        
        .send-button {
          padding: 8px 16px;
          background: #3182ce;
          border: none;
          border-radius: 6px;
          color: white;
          cursor: pointer;
          font-weight: 600;
        }
        
        .send-button:hover {
          background: #2c5aa0;
        }
      </style>
      
      <div class="chat-header">
        💬 Chat Widget
      </div>
      
      <div class="messages" id="messages">
        <div class="message assistant">
          Hello! This is a JTAG-powered chat widget. Try sending a message.
        </div>
      </div>
      
      <div class="input-area">
        <input type="text" class="message-input" id="messageInput" placeholder="Type a message...">
        <button class="send-button" id="sendButton">Send</button>
      </div>
    `;
  }

  private setupEventListeners() {
    const sendButton = this.shadowRoot?.getElementById('sendButton');
    const messageInput = this.shadowRoot?.getElementById('messageInput') as HTMLInputElement;
    
    if (sendButton && messageInput) {
      this.inputElement = messageInput;
      
      sendButton.addEventListener('click', () => this.sendMessage());
      messageInput.addEventListener('keypress', (e) => {
        if (e.key === 'Enter') {
          this.sendMessage();
        }
      });
    }
  }

  private async sendMessage() {
    if (!this.inputElement) return;
    
    const content = this.inputElement.value.trim();
    if (!content) return;

    // Add user message to UI
    this.addMessage(content, 'user');
    this.inputElement.value = '';

    try {
      // Use JTAG command system for chat functionality
      const response = await this.executeCommand('exec', {
        code: {
          type: 'inline',
          language: 'javascript',
          source: `
            // Simple echo response for now
            const response = {
              content: "Echo: " + ${JSON.stringify(content)},
              timestamp: new Date().toISOString()
            };
            response;
          `
        }
      });

      if (response.success && response.commandResult?.result?.content) {
        this.addMessage(response.commandResult.result.content, 'assistant');
      } else {
        this.addMessage('Sorry, I encountered an error processing your message.', 'assistant');
      }
      
    } catch (error) {
      console.error('Chat command failed:', error);
      this.addMessage('Sorry, I encountered an error. Please try again.', 'assistant');
    }
  }

  private addMessage(content: string, type: 'user' | 'assistant') {
    const messagesContainer = this.shadowRoot?.getElementById('messages');
    if (!messagesContainer) return;

    const messageElement = document.createElement('div');
    messageElement.className = `message ${type}`;
    messageElement.textContent = content;
    
    messagesContainer.appendChild(messageElement);
    messagesContainer.scrollTop = messagesContainer.scrollHeight;

    // Store message in memory
    this.messages.push({
      id: `msg_${Date.now()}`,
      content,
      type,
      timestamp: new Date()
    });
  }
}

// Widget will be registered dynamically by the generator system