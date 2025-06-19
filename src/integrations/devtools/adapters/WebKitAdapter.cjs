/**
 * WebKit Remote Debugging Adapter
 * For Safari and other WebKit-based browsers
 */

const BaseAdapter = require('./BaseAdapter.cjs');
const WebSocket = require('ws');

class WebKitAdapter extends BaseAdapter {
  constructor(options = {}) {
    super(options);
    this.port = options.port || 9999; // Safari Remote Debugging port
    this.ws = null;
    this.messageId = 1;
    this.pendingCommands = new Map();
  }

  async isAvailable() {
    try {
      // WebKit uses different discovery mechanism
      const response = await fetch(`http://localhost:${this.port}/json`);
      const targets = await response.json();
      return Array.isArray(targets);
    } catch (error) {
      return false;
    }
  }

  async connect() {
    try {
      const response = await fetch(`http://localhost:${this.port}/json`);
      const targets = await response.json();
      
      const target = targets.find(t => 
        t.type === 'page' && 
        (t.url.includes('localhost:9000') || t.title.includes('continuum'))
      );
      
      if (!target) {
        console.log('🔌 WebKit: No target found');
        return false;
      }

      this.ws = new WebSocket(target.webSocketDebuggerUrl);
      
      return new Promise((resolve, reject) => {
        this.ws.on('open', async () => {
          this.connected = true;
          await this.enableDomains();
          this.emit('connected');
          resolve(true);
        });

        this.ws.on('message', (data) => this.handleMessage(data));
        this.ws.on('error', reject);
      });
    } catch (error) {
      return false;
    }
  }

  async enableDomains() {
    // WebKit protocol is similar but may have differences
    await this.send('Console.enable');
    await this.send('Runtime.enable');
    await this.send('Page.enable');
  }

  async send(method, params = {}) {
    return new Promise((resolve, reject) => {
      const id = this.messageId++;
      this.pendingCommands.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }

  handleMessage(data) {
    const message = JSON.parse(data);
    
    if (message.id && this.pendingCommands.has(message.id)) {
      const pending = this.pendingCommands.get(message.id);
      this.pendingCommands.delete(message.id);
      pending.resolve(message.result);
      return;
    }
    
    // Handle WebKit-specific events
    this.handleWebKitEvent(message);
  }

  handleWebKitEvent(message) {
    const { method, params } = message;
    
    switch (method) {
      case 'Console.messageAdded':
        this.emitData('console-log', {
          level: params.message.level,
          text: params.message.text,
          source: 'webkit-console'
        });
        break;
        
      case 'Runtime.consoleAPICalled':
        this.emitData('console-api', {
          type: params.type,
          args: params.args.map(arg => arg.value || '[object]')
        });
        break;
    }
  }

  async takeScreenshot() {
    try {
      const result = await this.send('Page.captureScreenshot', {
        format: 'png',
        quality: 90
      });
      
      return {
        success: true,
        data: result.data,
        format: 'png'
      };
    } catch (error) {
      return { success: false, error: error.message };
    }
  }
}

module.exports = WebKitAdapter;