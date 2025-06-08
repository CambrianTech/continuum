/**
 * AIWidget - Simple React-like base class for AI-controlled widgets
 * Minimal, clean component system for AI-designed interfaces
 */

class AIWidget extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    this.state = {};
    this.wsConnection = null;
  }

  // React-like setState
  setState(newState) {
    this.state = { ...this.state, ...newState };
    this.render();
  }

  // Connect to WebSocket for AI commands
  connectWebSocket() {
    if (this.wsConnection) return;
    
    const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = `${protocol}//${location.host}`;
    
    this.wsConnection = new WebSocket(wsUrl);
    
    this.wsConnection.onopen = () => {
      console.log(`🔌 ${this.constructor.name} connected to WebSocket`);
      this.onWebSocketOpen();
    };
    
    this.wsConnection.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        this.handleWebSocketMessage(data);
      } catch (e) {
        console.warn('Invalid WebSocket message:', event.data);
      }
    };
    
    this.wsConnection.onclose = () => {
      console.log(`🔌 ${this.constructor.name} WebSocket closed, reconnecting...`);
      setTimeout(() => this.connectWebSocket(), 5000);
    };
  }

  // Override in child classes
  onWebSocketOpen() {}
  
  // Handle AI commands
  handleWebSocketMessage(data) {
    if (data.type === 'widget_refresh' && data.widget === this.constructor.name) {
      this.render();
    }
    
    if (data.type === 'widget_update' && data.widget === this.constructor.name) {
      this.setState(data.state);
    }
    
    if (data.type === 'widget_style' && data.widget === this.constructor.name) {
      this.applyStyles(data.styles);
    }
  }

  // Apply dynamic styles from AI
  applyStyles(styles) {
    Object.entries(styles).forEach(([property, value]) => {
      this.shadowRoot.host.style[property] = value;
    });
  }

  // Simple JSX-like template system
  html(template, ...values) {
    const html = template.reduce((result, string, i) => {
      const value = values[i] ? values[i] : '';
      return result + string + value;
    }, '');
    
    this.shadowRoot.innerHTML = html;
    this.attachEventListeners();
  }

  // Override in child classes
  render() {
    throw new Error('render() must be implemented');
  }

  // Override in child classes  
  attachEventListeners() {}

  // Lifecycle
  connectedCallback() {
    this.render();
    this.connectWebSocket();
  }

  disconnectedCallback() {
    if (this.wsConnection) {
      this.wsConnection.close();
    }
  }
}

// Export for use
window.AIWidget = AIWidget;