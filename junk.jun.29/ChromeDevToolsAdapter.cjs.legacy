/**
 * Chrome DevTools Protocol Adapter
 * Connects to Chrome DevTools Protocol for console logs and WebSocket monitoring
 */

const BaseAdapter = require('./BaseAdapter.cjs');
const WebSocket = require('ws');

class ChromeDevToolsAdapter extends BaseAdapter {
  constructor(options = {}) {
    super(options);
    this.port = options.port || 9222;
    this.targetUrl = options.targetUrl || 'localhost:9000';
    this.ws = null;
    this.messageId = 1;
    this.pendingCommands = new Map();
  }

  async isAvailable() {
    try {
      const response = await fetch(`http://localhost:${this.port}/json`);
      const targets = await response.json();
      return Array.isArray(targets);
    } catch (error) {
      return false;
    }
  }

  async connect() {
    try {
      // Get available targets
      const response = await fetch(`http://localhost:${this.port}/json`);
      const targets = await response.json();
      
      // Find Continuum page
      const target = targets.find(t => 
        t.type === 'page' && 
        (t.url.includes(this.targetUrl) || t.title.includes('continuum'))
      );
      
      if (!target) {
        console.log(`🔌 Chrome DevTools: No target found for ${this.targetUrl}`);
        return false;
      }

      console.log(`🔌 Chrome DevTools: Connecting to target: ${target.title}`);
      
      // Connect to WebSocket
      this.ws = new WebSocket(target.webSocketDebuggerUrl);
      
      return new Promise((resolve, reject) => {
        const timeout = setTimeout(() => {
          reject(new Error('Connection timeout'));
        }, 5000);

        this.ws.on('open', async () => {
          clearTimeout(timeout);
          console.log('🔌 Chrome DevTools: Connected to protocol');
          this.connected = true;
          
          // Enable required domains
          await this.enableDomains();
          
          this.emit('connected');
          resolve(true);
        });

        this.ws.on('message', (data) => {
          this.handleMessage(data);
        });

        this.ws.on('error', (error) => {
          clearTimeout(timeout);
          this.emitError(error, { phase: 'connection' });
          reject(error);
        });

        this.ws.on('close', () => {
          this.connected = false;
          this.emit('disconnected');
        });
      });
    } catch (error) {
      this.emitError(error, { phase: 'setup' });
      return false;
    }
  }

  async enableDomains() {
    const domains = ['Log', 'Runtime', 'Network'];
    
    for (const domain of domains) {
      await this.send(`${domain}.enable`);
    }
    
    console.log('🔌 Chrome DevTools: Enabled domains:', domains.join(', '));
  }

  async send(method, params = {}) {
    if (!this.connected || !this.ws) {
      throw new Error('Not connected to Chrome DevTools');
    }

    return new Promise((resolve, reject) => {
      const id = this.messageId++;
      const message = { id, method, params };
      
      // Store pending command
      this.pendingCommands.set(id, { resolve, reject, method });
      
      // Set timeout
      setTimeout(() => {
        if (this.pendingCommands.has(id)) {
          this.pendingCommands.delete(id);
          reject(new Error(`Command timeout: ${method}`));
        }
      }, 5000);
      
      this.ws.send(JSON.stringify(message));
    });
  }

  handleMessage(data) {
    try {
      const message = JSON.parse(data);
      
      // Handle command responses
      if (message.id && this.pendingCommands.has(message.id)) {
        const pending = this.pendingCommands.get(message.id);
        this.pendingCommands.delete(message.id);
        
        if (message.error) {
          pending.reject(new Error(`${pending.method}: ${message.error.message}`));
        } else {
          pending.resolve(message.result);
        }
        return;
      }
      
      // Handle events
      this.handleEvent(message);
      
    } catch (error) {
      this.emitError(error, { phase: 'message-parsing', data: data.toString() });
    }
  }

  handleEvent(message) {
    const { method, params } = message;
    
    switch (method) {
      case 'Log.entryAdded':
        this.emitData('console-log', {
          level: params.entry.level,
          text: params.entry.text,
          source: params.entry.source,
          timestamp: new Date(params.entry.timestamp).toISOString()
        });
        break;
        
      case 'Runtime.consoleAPICalled':
        this.emitData('console-api', {
          type: params.type,
          args: params.args.map(arg => arg.value || arg.description || '[object]'),
          timestamp: new Date(params.timestamp).toISOString(),
          stackTrace: params.stackTrace
        });
        break;
        
      case 'Network.webSocketCreated':
        this.emitData('websocket-created', {
          requestId: params.requestId,
          url: params.url
        });
        break;
        
      case 'Network.webSocketFrameReceived':
        this.emitData('websocket-frame', {
          direction: 'received',
          requestId: params.requestId,
          opcode: params.response.opcode,
          mask: params.response.mask,
          payloadData: params.response.payloadData
        });
        break;
        
      case 'Network.webSocketFrameSent':
        this.emitData('websocket-frame', {
          direction: 'sent',
          requestId: params.requestId,
          opcode: params.response.opcode,
          mask: params.response.mask,
          payloadData: params.response.payloadData
        });
        break;
        
      default:
        // Ignore unknown events
        break;
    }
  }

  async disconnect() {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
    
    // Clear pending commands
    for (const [id, pending] of this.pendingCommands) {
      pending.reject(new Error('Connection closed'));
    }
    this.pendingCommands.clear();
    
    await super.disconnect();
  }
}

module.exports = ChromeDevToolsAdapter;