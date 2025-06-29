/**
 * Status Pill Web Component
 * Shows connection status with animated indicator
 */

class StatusPill extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    
    // Component state
    this.status = 'online';
    this.text = 'Connected';
    this.showPulse = true;
  }

  connectedCallback() {
    this.render();
  }

  static get observedAttributes() {
    return ['status', 'text', 'pulse'];
  }

  attributeChangedCallback(name, oldValue, newValue) {
    switch (name) {
      case 'status':
        this.status = newValue;
        break;
      case 'text':
        this.text = newValue;
        break;
      case 'pulse':
        this.showPulse = newValue !== 'false';
        break;
    }
    if (this.shadowRoot.innerHTML) {
      this.render();
    }
  }

  getStatusColor() {
    switch (this.status) {
      case 'online':
      case 'connected':
        return '#4CAF50';
      case 'connecting':
      case 'pending':
        return '#FF9800';
      case 'offline':
      case 'disconnected':
        return '#F44336';
      case 'warning':
        return '#FFC107';
      default:
        return '#666';
    }
  }

  render() {
    const color = this.getStatusColor();
    
    this.shadowRoot.innerHTML = `
      <style>
        :host {
          display: inline-flex;
        }

        .status-pill {
          display: inline-flex;
          align-items: center;
          padding: 6px 12px;
          background: rgba(${this.hexToRgb(color)}, 0.1);
          border: 1px solid rgba(${this.hexToRgb(color)}, 0.3);
          border-radius: 20px;
          font-size: 12px;
          color: ${color};
          font-weight: 500;
        }

        .status-dot {
          width: 6px;
          height: 6px;
          background: ${color};
          border-radius: 50%;
          margin-right: 8px;
          ${this.showPulse ? 'animation: pulse 2s infinite;' : ''}
        }

        @keyframes pulse {
          0%, 100% { 
            opacity: 1; 
            transform: scale(1);
          }
          50% { 
            opacity: 0.5; 
            transform: scale(0.9);
          }
        }

        .status-text {
          font-size: 12px;
          line-height: 1;
        }
      </style>

      <div class="status-pill">
        <div class="status-dot"></div>
        <div class="status-text">${this.text}</div>
      </div>
    `;
  }

  hexToRgb(hex) {
    const result = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex);
    return result ? 
      `${parseInt(result[1], 16)}, ${parseInt(result[2], 16)}, ${parseInt(result[3], 16)}` : 
      '255, 255, 255';
  }

  // Public API
  setStatus(status, text = null) {
    this.status = status;
    if (text) {
      this.text = text;
    }
    this.render();
  }

  setPulse(enabled) {
    this.showPulse = enabled;
    this.render();
  }
}

// Register the custom element
if (typeof customElements !== 'undefined') {
  customElements.define('status-pill', StatusPill);
}

// Export for Node.js
if (typeof module !== 'undefined' && module.exports) {
  module.exports = StatusPill;
}