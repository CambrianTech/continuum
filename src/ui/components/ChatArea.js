/**
 * Chat Area Web Component
 * Main chat interface with messages and input
 */

class ChatArea extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    
    // Component state
    this.messages = [];
    this.isConnected = false;
    this.onSendMessage = null;
  }

  connectedCallback() {
    this.render();
    this.setupEventListeners();
  }

  setupEventListeners() {
    this.shadowRoot.addEventListener('click', (e) => {
      if (e.target.closest('.send-btn')) {
        this.handleSendMessage();
      }
    });

    this.shadowRoot.addEventListener('keydown', (e) => {
      if (e.target.closest('.message-input') && e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        this.handleSendMessage();
      }
    });
  }

  handleSendMessage() {
    const input = this.shadowRoot.querySelector('.message-input');
    const message = input.value.trim();
    
    if (!message || !this.isConnected) return;
    
    if (this.onSendMessage) {
      this.onSendMessage(message);
    }
    
    this.dispatchEvent(new CustomEvent('send-message', {
      detail: { message },
      bubbles: true
    }));
    
    input.value = '';
    this.updateSendButton();
  }

  updateSendButton() {
    const input = this.shadowRoot.querySelector('.message-input');
    const sendBtn = this.shadowRoot.querySelector('.send-btn');
    
    if (input && sendBtn) {
      const hasText = input.value.trim().length > 0;
      sendBtn.disabled = !hasText || !this.isConnected;
      sendBtn.style.opacity = (hasText && this.isConnected) ? '1' : '0.5';
    }
  }

  addMessage(message) {
    this.messages.push(message);
    this.updateMessagesDisplay();
    this.scrollToBottom();
  }

  updateMessagesDisplay() {
    const messagesContainer = this.shadowRoot.querySelector('.messages');
    if (!messagesContainer) return;
    
    messagesContainer.innerHTML = this.messages.map(msg => `
      <div class="message ${msg.type}">
        <div class="message-header">
          <span class="message-sender">${msg.sender}</span>
          <span class="message-time">${msg.timestamp}</span>
        </div>
        <div class="message-content">${msg.content}</div>
      </div>
    `).join('');
  }

  scrollToBottom() {
    const messagesContainer = this.shadowRoot.querySelector('.messages');
    if (messagesContainer) {
      messagesContainer.scrollTop = messagesContainer.scrollHeight;
    }
  }

  render() {
    this.shadowRoot.innerHTML = `
      <style>
        :host {
          display: flex;
          flex-direction: column;
          height: 100%;
          background: rgba(15, 20, 25, 0.5);
        }

        .messages {
          flex: 1;
          padding: 20px;
          overflow-y: auto;
          scroll-behavior: smooth;
        }

        .message {
          margin-bottom: 15px;
          padding: 12px 16px;
          border-radius: 12px;
          max-width: 80%;
          word-wrap: break-word;
        }

        .message.user {
          background: linear-gradient(135deg, #4FC3F7, #29B6F6);
          margin-left: auto;
          color: white;
        }

        .message.assistant {
          background: rgba(255, 255, 255, 0.05);
          border: 1px solid rgba(255, 255, 255, 0.1);
          color: #e0e6ed;
        }

        .message.system {
          background: rgba(255, 193, 7, 0.1);
          border: 1px solid rgba(255, 193, 7, 0.3);
          color: #FFC107;
          font-style: italic;
          text-align: center;
          margin: 10px auto;
        }

        .message-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-bottom: 6px;
          font-size: 12px;
          opacity: 0.7;
        }

        .message-sender {
          font-weight: 600;
        }

        .message-time {
          font-size: 11px;
        }

        .message-content {
          line-height: 1.5;
          white-space: pre-wrap;
        }

        .input-area {
          padding: 20px;
          border-top: 1px solid rgba(255, 255, 255, 0.1);
          background: rgba(20, 25, 35, 0.8);
          backdrop-filter: blur(10px);
        }

        .input-container {
          display: flex;
          gap: 12px;
          align-items: flex-end;
        }

        .message-input {
          flex: 1;
          background: rgba(255, 255, 255, 0.05);
          border: 1px solid rgba(255, 255, 255, 0.1);
          border-radius: 12px;
          padding: 12px 16px;
          color: #e0e6ed;
          font-size: 14px;
          resize: none;
          min-height: 44px;
          max-height: 120px;
          font-family: inherit;
        }

        .message-input:focus {
          outline: none;
          border-color: #4FC3F7;
          box-shadow: 0 0 0 2px rgba(79, 195, 247, 0.2);
        }

        .message-input:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }

        .message-input::placeholder {
          color: #8a92a5;
        }

        .send-btn {
          background: linear-gradient(135deg, #4FC3F7, #29B6F6);
          border: none;
          border-radius: 12px;
          padding: 12px 20px;
          color: white;
          font-weight: 600;
          cursor: pointer;
          transition: all 0.2s ease;
          display: flex;
          align-items: center;
          gap: 6px;
          white-space: nowrap;
        }

        .send-btn:hover:not(:disabled) {
          transform: translateY(-1px);
          box-shadow: 0 4px 12px rgba(79, 195, 247, 0.3);
        }

        .send-btn:disabled {
          opacity: 0.5;
          cursor: not-allowed;
          transform: none;
        }

        .empty-state {
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          height: 100%;
          color: #8a92a5;
          text-align: center;
        }

        .empty-icon {
          font-size: 48px;
          margin-bottom: 16px;
        }

        .empty-title {
          font-size: 18px;
          font-weight: 600;
          margin-bottom: 8px;
        }

        .empty-subtitle {
          font-size: 14px;
          opacity: 0.7;
        }

        ::-webkit-scrollbar {
          width: 6px;
        }

        ::-webkit-scrollbar-track {
          background: rgba(255, 255, 255, 0.05);
          border-radius: 3px;
        }

        ::-webkit-scrollbar-thumb {
          background: rgba(79, 195, 247, 0.3);
          border-radius: 3px;
        }
      </style>

      <div class="messages">
        ${this.messages.length === 0 ? `
          <div class="empty-state">
            <div class="empty-icon">💬</div>
            <div class="empty-title">Start a conversation</div>
            <div class="empty-subtitle">Send a message to begin chatting with your AI agents</div>
          </div>
        ` : ''}
      </div>

      <div class="input-area">
        <div class="input-container">
          <textarea 
            class="message-input" 
            placeholder="${this.isConnected ? 'Type your message...' : 'Connecting...'}"
            ${this.isConnected ? '' : 'disabled'}
          ></textarea>
          <button class="send-btn" ${this.isConnected ? '' : 'disabled'}>
            <span>Send</span>
            <span>↗</span>
          </button>
        </div>
      </div>
    `;

    // Setup input event listener for send button state
    const input = this.shadowRoot.querySelector('.message-input');
    if (input) {
      input.addEventListener('input', () => this.updateSendButton());
    }
  }

  // Public API
  setConnected(connected) {
    this.isConnected = connected;
    this.render();
  }

  setMessages(messages) {
    this.messages = messages;
    this.updateMessagesDisplay();
    this.scrollToBottom();
  }

  clearMessages() {
    this.messages = [];
    this.render();
  }

  setOnSendMessage(callback) {
    this.onSendMessage = callback;
  }
}

// Register the custom element
if (typeof customElements !== 'undefined') {
  customElements.define('chat-area', ChatArea);
}

// Export for Node.js
if (typeof module !== 'undefined' && module.exports) {
  module.exports = ChatArea;
}