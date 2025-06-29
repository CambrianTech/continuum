/**
 * DevTools Integration
 * Pluggable system for capturing browser console logs and WebSocket traffic
 */

const WebSocket = require('ws');
const { EventEmitter } = require('events');

class DevToolsIntegration extends EventEmitter {
  constructor(options = {}) {
    super();
    this.enabled = options.enabled !== false;
    this.port = options.port || 9222; // Default Chrome DevTools port
    this.methods = new Map();
    this.ws = null;
    this.connected = false;
    this.messageId = 1;
    
    // Register built-in capture methods
    this.registerMethod('chrome-devtools-protocol', this.chromeDevToolsProtocol.bind(this));
    this.registerMethod('console-injection', this.consoleInjection.bind(this));
    this.registerMethod('network-proxy', this.networkProxy.bind(this));
  }

  /**
   * Register a new capture method
   */
  registerMethod(name, handler) {
    this.methods.set(name, handler);
    console.log(`🔌 DevTools: Registered capture method: ${name}`);
  }

  /**
   * Start capturing with preferred method
   */
  async startCapture(method = 'chrome-devtools-protocol') {
    if (!this.enabled) {
      console.log('🔌 DevTools: Integration disabled');
      return false;
    }

    const handler = this.methods.get(method);
    if (!handler) {
      console.error(`🔌 DevTools: Unknown method: ${method}`);
      return false;
    }

    console.log(`🔌 DevTools: Starting capture with method: ${method}`);
    return await handler();
  }

  /**
   * Method 1: Chrome DevTools Protocol
   * Most reliable for console logs and WebSocket metadata
   */
  async chromeDevToolsProtocol() {
    try {
      // Connect to Chrome DevTools Protocol
      const wsUrl = `ws://localhost:${this.port}/json`;
      
      // First get available targets
      const response = await fetch(`http://localhost:${this.port}/json`);
      const targets = await response.json();
      
      const target = targets.find(t => t.type === 'page' && t.url.includes('localhost:9000'));
      if (!target) {
        console.log('🔌 DevTools: No Continuum page found');
        return false;
      }

      this.ws = new WebSocket(target.webSocketDebuggerUrl);
      
      this.ws.on('open', () => {
        console.log('🔌 DevTools: Connected to Chrome DevTools Protocol');
        this.connected = true;
        
        // Enable console logging
        this.send('Log.enable');
        this.send('Runtime.enable');
        this.send('Network.enable');
        
        this.emit('connected');
      });

      this.ws.on('message', (data) => {
        try {
          const message = JSON.parse(data);
          this.handleDevToolsMessage(message);
        } catch (error) {
          console.error('🔌 DevTools: Message parse error:', error);
        }
      });

      this.ws.on('error', (error) => {
        console.error('🔌 DevTools: Connection error:', error);
        this.emit('error', error);
      });

      return true;
    } catch (error) {
      console.error('🔌 DevTools: Failed to connect:', error);
      return false;
    }
  }

  /**
   * Method 2: Console Injection
   * Inject JavaScript to override console methods and WebSocket constructor
   */
  async consoleInjection() {
    const injectionScript = `
      (function() {
        // Override console methods
        const originalLog = console.log;
        const originalError = console.error;
        const originalWarn = console.warn;
        
        const sendToCapture = (level, args) => {
          if (window.continuumDevToolsCapture) {
            window.continuumDevToolsCapture({
              type: 'console',
              level: level,
              message: args.map(arg => String(arg)).join(' '),
              timestamp: new Date().toISOString()
            });
          }
        };
        
        console.log = function(...args) {
          originalLog.apply(console, args);
          sendToCapture('log', args);
        };
        
        console.error = function(...args) {
          originalError.apply(console, args);
          sendToCapture('error', args);
        };
        
        console.warn = function(...args) {
          originalWarn.apply(console, args);
          sendToCapture('warn', args);
        };
        
        // Override WebSocket constructor
        const OriginalWebSocket = window.WebSocket;
        window.WebSocket = function(url, protocols) {
          const ws = new OriginalWebSocket(url, protocols);
          const originalSend = ws.send;
          
          ws.send = function(data) {
            if (window.continuumDevToolsCapture) {
              window.continuumDevToolsCapture({
                type: 'websocket',
                direction: 'sent',
                data: data,
                url: url,
                timestamp: new Date().toISOString()
              });
            }
            return originalSend.call(this, data);
          };
          
          ws.addEventListener('message', function(event) {
            if (window.continuumDevToolsCapture) {
              window.continuumDevToolsCapture({
                type: 'websocket',
                direction: 'received',
                data: event.data,
                url: url,
                timestamp: new Date().toISOString()
              });
            }
          });
          
          return ws;
        };
        
        // Copy static properties
        Object.setPrototypeOf(window.WebSocket, OriginalWebSocket);
        window.WebSocket.prototype = OriginalWebSocket.prototype;
        
        console.log('🔌 DevTools: Console injection active');
      })();
    `;

    // This would need to be injected into the browser page
    // Could be done via continuum's existing JavaScript execution system
    console.log('🔌 DevTools: Console injection method prepared');
    this.emit('injection-ready', injectionScript);
    return true;
  }

  /**
   * Method 3: Network Proxy
   * Intercept traffic at network level
   */
  async networkProxy() {
    console.log('🔌 DevTools: Network proxy method not yet implemented');
    // Could implement HTTP/WebSocket proxy here
    return false;
  }

  /**
   * Handle messages from Chrome DevTools Protocol
   */
  handleDevToolsMessage(message) {
    if (message.method === 'Log.entryAdded') {
      this.emit('console-log', {
        type: 'console',
        level: message.params.entry.level,
        text: message.params.entry.text,
        timestamp: new Date(message.params.entry.timestamp).toISOString(),
        source: message.params.entry.source
      });
    }
    
    if (message.method === 'Runtime.consoleAPICalled') {
      this.emit('console-log', {
        type: 'console',
        level: message.params.type,
        args: message.params.args.map(arg => arg.value || arg.description),
        timestamp: new Date(message.params.timestamp).toISOString()
      });
    }
    
    if (message.method === 'Network.webSocketFrameReceived') {
      this.emit('websocket-frame', {
        type: 'websocket',
        direction: 'received',
        requestId: message.params.requestId,
        frame: message.params.response
      });
    }
    
    if (message.method === 'Network.webSocketFrameSent') {
      this.emit('websocket-frame', {
        type: 'websocket',
        direction: 'sent',
        requestId: message.params.requestId,
        frame: message.params.response
      });
    }
  }

  /**
   * Send command to DevTools Protocol
   */
  send(method, params = {}) {
    if (!this.connected || !this.ws) {
      console.warn('🔌 DevTools: Not connected');
      return;
    }

    const message = {
      id: this.messageId++,
      method: method,
      params: params
    };

    this.ws.send(JSON.stringify(message));
  }

  /**
   * Stop capturing
   */
  stop() {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
      this.connected = false;
    }
    console.log('🔌 DevTools: Capture stopped');
    this.emit('stopped');
  }

  /**
   * Inject script into browser page
   */
  async injectScript(script) {
    if (!this.connected) {
      console.warn('🔌 DevTools: Not connected for injection');
      return false;
    }

    this.send('Runtime.evaluate', {
      expression: script,
      includeCommandLineAPI: true
    });

    return true;
  }
}

module.exports = DevToolsIntegration;