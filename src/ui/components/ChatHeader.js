/**
 * Chat Header Web Component
 * Displays chat title, controls, and status
 */

class ChatHeader extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    
    // Component state
    this.chatTitle = 'Agent Coordination';
    this.chatSubtitle = 'Multi-agent AI workspace';
    this.onClearChat = null;
  }

  connectedCallback() {
    this.render();
    this.setupEventListeners();
  }

  static get observedAttributes() {
    return ['title', 'subtitle'];
  }

  attributeChangedCallback(name, oldValue, newValue) {
    switch (name) {
      case 'title':
        this.chatTitle = newValue;
        break;
      case 'subtitle':
        this.chatSubtitle = newValue;
        break;
    }
    if (this.shadowRoot.innerHTML) {
      this.render();
    }
  }

  setupEventListeners() {
    this.shadowRoot.addEventListener('click', (e) => {
      if (e.target.closest('.clear-chat-btn')) {
        if (this.onClearChat) {
          this.onClearChat();
        }
        
        this.dispatchEvent(new CustomEvent('clear-chat', {
          bubbles: true
        }));
      }
    });
  }

  render() {
    this.shadowRoot.innerHTML = `
      <style>
        :host {
          display: block;
          padding: 20px 30px;
          border-bottom: 1px solid rgba(255, 255, 255, 0.1);
          background: rgba(20, 25, 35, 0.8);
          backdrop-filter: blur(10px);
        }

        .header-container {
          display: flex;
          align-items: flex-start;
          justify-content: space-between;
        }

        .header-content {
          flex: 1;
        }

        .chat-title {
          font-size: 20px;
          font-weight: 600;
          color: #e0e6ed;
          margin-bottom: 5px;
        }

        .chat-subtitle {
          font-size: 14px;
          color: #8a92a5;
        }

        .clear-chat-btn {
          background: rgba(244, 67, 54, 0.2);
          border: 1px solid #F44336;
          color: #F44336;
          border-radius: 6px;
          padding: 8px 12px;
          font-size: 12px;
          cursor: pointer;
          transition: all 0.2s ease;
          display: flex;
          align-items: center;
          gap: 5px;
          white-space: nowrap;
        }

        .clear-chat-btn:hover {
          background: rgba(244, 67, 54, 0.3);
          transform: translateY(-1px);
        }

        .clear-chat-btn:active {
          transform: translateY(0);
        }
      </style>

      <div class="header-container">
        <div class="header-content">
          <div class="chat-title">${this.chatTitle}</div>
          <div class="chat-subtitle">${this.chatSubtitle}</div>
        </div>
        <button class="clear-chat-btn">
          🗑️ Clear Chat
        </button>
      </div>
    `;
  }

  // Public API
  setTitle(title) {
    this.chatTitle = title;
    this.render();
  }

  setSubtitle(subtitle) {
    this.chatSubtitle = subtitle;
    this.render();
  }

  setOnClearChat(callback) {
    this.onClearChat = callback;
  }
}

// Register the custom element
if (typeof customElements !== 'undefined') {
  customElements.define('chat-header', ChatHeader);
}

// Export for Node.js
if (typeof module !== 'undefined' && module.exports) {
  module.exports = ChatHeader;
}