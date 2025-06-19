/**
 * Client Connection Manager
 * Manages connected browser clients and their DevTools integration
 */

const BrowserDetector = require('./BrowserDetector.cjs');
const OperaLauncher = require('./OperaLauncher.cjs');
const DevToolsConnector = require('./DevToolsConnector.cjs');

// Import the existing Python DevTools architecture concepts
// Map to our existing browser adapter pattern

class ClientManager {
  constructor(continuum) {
    this.continuum = continuum;
    this.clients = new Map(); // sessionId -> client info
    this.devtools = new DevToolsConnector();
    this.operaLauncher = new OperaLauncher();
  }

  async addClient(sessionId, ws) {
    // Extract browser info from WebSocket connection
    const connectionInfo = BrowserDetector.extractClientInfo(ws);
    const browserInfo = BrowserDetector.detectFromUserAgent(connectionInfo.userAgent);
    
    const client = {
      sessionId,
      ...connectionInfo,
      browserType: browserInfo.type,
      devToolsPort: browserInfo.port,
      devToolsConnected: false
    };

    this.clients.set(sessionId, client);
    
    console.log(`🌐 Client connected: ${browserInfo.type} (${connectionInfo.userAgent})`);
    
    // Try to connect to DevTools
    await this.connectDevTools(sessionId);
  }

  async connectDevTools(sessionId) {
    const client = this.clients.get(sessionId);
    if (!client) return;

    try {
      // Try existing DevTools connection
      const browserInfo = await this.devtools.tryConnect(client.devToolsPort);
      
      if (browserInfo) {
        console.log(`🔌 Found existing ${client.browserType} DevTools`);
        await this.establishConnection(sessionId, browserInfo.target);
        return;
      }

      // Launch browser if it's Opera
      if (client.browserType === 'opera') {
        await this.launchOpera(sessionId);
      } else {
        this.notifyUserToLaunchBrowser(client);
      }
    } catch (error) {
      console.log(`🔌 DevTools connection failed for ${sessionId}:`, error.message);
    }
  }

  async launchOpera(sessionId) {
    const client = this.clients.get(sessionId);
    
    try {
      console.log(`🚀 Launching Opera with DevTools...`);
      const result = this.operaLauncher.launch(client.devToolsPort);
      
      this.continuum.continuonStatus.updateStatusText(
        `🚀 Launching Opera with DevTools...`,
        { duration: 3000 }
      );

      // Wait and retry connection
      setTimeout(async () => {
        await this.connectDevTools(sessionId);
      }, 3000);

    } catch (error) {
      console.log(`🔌 Failed to launch Opera:`, error.message);
      this.notifyUserToLaunchBrowser(client);
    }
  }

  async establishConnection(sessionId, target) {
    try {
      const ws = await this.devtools.connect(sessionId, target);
      const client = this.clients.get(sessionId);
      
      client.devToolsConnected = true;
      
      console.log(`🔌 DevTools connected for ${client.browserType}`);
      this.continuum.continuonStatus.updateStatusText(
        `🔌 ${client.browserType} console monitoring active`,
        { duration: 3000 }
      );

      // Handle console messages
      ws.on('message', (data) => {
        this.handleDevToolsMessage(sessionId, data);
      });

    } catch (error) {
      console.log(`🔌 Failed to establish DevTools connection:`, error.message);
    }
  }

  handleDevToolsMessage(sessionId, data) {
    try {
      const message = JSON.parse(data);
      
      if (message.method === 'Runtime.consoleAPICalled') {
        const args = message.params.args || [];
        const text = args.map(arg => arg.value || '[object]').join(' ');
        
        // Show important messages in status text
        if (message.params.type === 'error' || text.includes('🛰️')) {
          this.continuum.continuonStatus.updateStatusText(
            `🌐 ${message.params.type.toUpperCase()}: ${text}`,
            { duration: 5000 }
          );
        }
      }
    } catch (error) {
      console.log(`🔌 Error parsing DevTools message:`, error.message);
    }
  }

  notifyUserToLaunchBrowser(client) {
    this.continuum.continuonStatus.updateStatusText(
      `🔌 Start ${client.browserType} with --remote-debugging-port=${client.devToolsPort}`,
      { duration: 10000 }
    );
  }

  removeClient(sessionId) {
    this.devtools.disconnect(sessionId);
    this.clients.delete(sessionId);
  }

  getClientInfo() {
    return Array.from(this.clients.values());
  }
}

module.exports = ClientManager;