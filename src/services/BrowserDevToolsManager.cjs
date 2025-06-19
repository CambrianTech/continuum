/**
 * Browser DevTools Manager
 * Simplified integration with existing DevTools architecture
 */

const BrowserDetector = require('./BrowserDetector.cjs');
const OperaLauncher = require('./OperaLauncher.cjs');
const DevToolsConnector = require('./DevToolsConnector.cjs');

class BrowserDevToolsManager {
  constructor(continuum) {
    this.continuum = continuum;
    this.clients = new Map(); // sessionId -> client info
    this.devtools = new DevToolsConnector();
    this.operaLauncher = new OperaLauncher();
  }

  /**
   * Add a new client and try to connect to their specific browser
   */
  async addClient(sessionId, clientInfo = {}) {
    const client = this.continuum.connectedClients.get(sessionId);
    if (client) {
      // Update existing client info
      Object.assign(client, clientInfo);
    }

    console.log(`🔌 DevTools: Attempting to connect to ${clientInfo.browserType} on port ${clientInfo.devToolsPort}`);

    // Try to connect to the detected browser
    await this.connectToSpecificBrowser(sessionId, clientInfo.browserType, clientInfo.devToolsPort);
  }

  /**
   * Remove client and disconnect browser monitoring
   */
  async removeClient(sessionId) {
    await this.disconnectBrowser(sessionId);
    this.continuum.connectedClients.delete(sessionId);
  }

  /**
   * Connect to specific browser type with intelligent launching
   */
  async connectToSpecificBrowser(sessionId, browserType, port) {
    try {
      // First, try to connect to existing DevTools
      const browserInfo = await this.tryConnectToBrowser(port);
      
      if (browserInfo) {
        console.log(`🔌 DevTools: Found existing ${browserType} with DevTools enabled`);
        await this.establishBrowserConnection(sessionId, browserInfo, port);
        return;
      }
      
      // If not found, try to launch browser with DevTools enabled
      console.log(`🔌 DevTools: ${browserType} DevTools not found, attempting to launch...`);
      
      if (browserType === 'opera') {
        await this.launchOperaWithDevTools(sessionId, port);
      } else {
        console.log(`🔌 DevTools: Auto-launch not implemented for ${browserType}`);
        this.continuum.continuonStatus.updateStatusText(
          `🔌 Please start ${browserType} with: --remote-debugging-port=${port}`,
          { duration: 10000 }
        );
      }
      
    } catch (error) {
      console.log(`🔌 DevTools: Error connecting to ${browserType}:`, error.message);
    }
  }

  /**
   * Launch Opera with DevTools enabled
   */
  async launchOperaWithDevTools(sessionId, port) {
    const { spawn } = require('child_process');
    
    try {
      console.log(`🔌 DevTools: Launching Opera with DevTools on port ${port}...`);
      
      // Opera GX/Opera paths to try
      const operaPaths = [
        '/Applications/Opera GX.app/Contents/MacOS/Opera',
        '/Applications/Opera.app/Contents/MacOS/Opera'
      ];
      
      let operaPath = null;
      const fs = require('fs');
      
      // Find existing Opera installation
      for (const path of operaPaths) {
        if (fs.existsSync(path)) {
          operaPath = path;
          break;
        }
      }
      
      if (!operaPath) {
        console.log(`🔌 DevTools: Opera not found in standard locations`);
        this.continuum.continuonStatus.updateStatusText(
          `🔌 Opera not found. Please start manually with --remote-debugging-port=${port}`,
          { duration: 10000 }
        );
        return;
      }
      
      // Launch Opera with DevTools
      const opera = spawn(operaPath, [
        `--remote-debugging-port=${port}`,
        '--disable-web-security',
        '--user-data-dir=/tmp/opera-devtools',
        'http://localhost:9000'
      ], {
        detached: true,
        stdio: 'ignore'
      });
      
      opera.unref();
      
      console.log(`🔌 DevTools: Opera launched with PID ${opera.pid}`);
      this.continuum.continuonStatus.updateStatusText(
        `🚀 Launching Opera with DevTools enabled...`,
        { duration: 5000 }
      );
      
      // Wait a bit for Opera to start, then try connecting
      setTimeout(async () => {
        await this.connectToSpecificBrowser(sessionId, 'opera', port);
      }, 3000);
      
    } catch (error) {
      console.log(`🔌 DevTools: Failed to launch Opera:`, error.message);
      this.continuum.continuonStatus.updateStatusText(
        `❌ Failed to launch Opera: ${error.message}`,
        { duration: 5000 }
      );
    }
  }

  /**
   * Try to connect to browser on specific port
   */
  async tryConnectToBrowser(port) {
    try {
      const response = await fetch(`http://localhost:${port}/json`, { timeout: 2000 });
      const targets = await response.json();
      
      // Look for Continuum page
      const continuumTarget = targets.find(target => 
        target.type === 'page' && 
        (target.url.includes('localhost:9000') || 
         target.title.toLowerCase().includes('continuum'))
      );

      if (continuumTarget) {
        return {
          target: continuumTarget,
          browserType: this.detectBrowserType(port),
          targets: targets
        };
      }
    } catch (error) {
      // Browser not available on this port
      return null;
    }
    
    return null;
  }

  /**
   * Establish WebSocket connection to browser DevTools
   */
  async establishBrowserConnection(sessionId, browserInfo, port) {
    try {
      const ws = new WebSocket(browserInfo.target.webSocketDebuggerUrl);
      
      ws.on('open', () => {
        console.log(`🔌 DevTools: Connected to ${browserInfo.browserType} for session ${sessionId}`);
        
        // Enable console logging
        ws.send(JSON.stringify({ id: 1, method: 'Log.enable', params: {} }));
        ws.send(JSON.stringify({ id: 2, method: 'Runtime.enable', params: {} }));
        
        // Update client info
        const client = this.continuum.connectedClients.get(sessionId);
        if (client) {
          client.browserDetected = true;
          client.browserType = browserInfo.browserType;
          client.devToolsPort = port;
          client.consoleLogsEnabled = true;
          
          // Notify client that browser monitoring is active
          this.continuum.continuonStatus.updateStatusText(
            `🔌 Browser console monitoring active (${browserInfo.browserType})`
          );
        }
      });

      ws.on('message', (data) => {
        this.handleBrowserMessage(sessionId, data);
      });

      ws.on('close', () => {
        console.log(`🔌 DevTools: Disconnected from browser for session ${sessionId}`);
        this.connectedBrowsers.delete(sessionId);
      });

      ws.on('error', (error) => {
        console.log(`🔌 DevTools: Browser connection error for ${sessionId}:`, error.message);
      });

      this.connectedBrowsers.set(sessionId, {
        ws,
        browserInfo,
        port
      });

    } catch (error) {
      console.log(`🔌 DevTools: Failed to establish browser connection for ${sessionId}:`, error.message);
    }
  }

  /**
   * Handle messages from browser DevTools
   */
  handleBrowserMessage(sessionId, data) {
    try {
      const message = JSON.parse(data);
      
      // Handle console logs
      if (message.method === 'Log.entryAdded') {
        this.handleConsoleLog(sessionId, message.params.entry);
      } else if (message.method === 'Runtime.consoleAPICalled') {
        this.handleConsoleAPI(sessionId, message.params);
      }
    } catch (error) {
      console.log(`🔌 DevTools: Error parsing browser message:`, error.message);
    }
  }

  /**
   * Handle console log entry
   */
  handleConsoleLog(sessionId, entry) {
    const logMessage = {
      type: 'browser_log',
      sessionId,
      level: entry.level || 'info',
      text: entry.text || '',
      source: entry.source || 'console',
      timestamp: new Date(entry.timestamp).toISOString(),
      browser: this.continuum.connectedClients.get(sessionId)?.browserType || 'unknown'
    };

    // Send to status text if it's important
    if (entry.level === 'error' || entry.text.includes('🛰️')) {
      this.continuum.continuonStatus.updateStatusText(
        `🌐 ${entry.level.toUpperCase()}: ${entry.text}`,
        { duration: 5000 }
      );
    }

    // Broadcast to EventBus for real-time monitoring
    this.continuum.eventBus.publish('browser_log', logMessage);
  }

  /**
   * Handle console API calls (console.log, console.error, etc.)
   */
  handleConsoleAPI(sessionId, params) {
    const args = params.args || [];
    const textParts = args.map(arg => 
      arg.value !== undefined ? String(arg.value) : 
      arg.description || '[object]'
    );
    
    const logMessage = {
      type: 'browser_console',
      sessionId,
      level: params.type || 'log',
      text: textParts.join(' '),
      source: 'console-api',
      timestamp: new Date(params.timestamp).toISOString(),
      browser: this.continuum.connectedClients.get(sessionId)?.browserType || 'unknown'
    };

    // Send important messages to status text
    if (params.type === 'error' || textParts.some(part => part.includes('🛰️'))) {
      this.continuum.continuonStatus.updateStatusText(
        `🌐 ${params.type.toUpperCase()}: ${textParts.join(' ')}`,
        { duration: 5000 }
      );
    }

    // Broadcast to EventBus
    this.continuum.eventBus.publish('browser_console', logMessage);
  }

  /**
   * Disconnect browser monitoring for session
   */
  async disconnectBrowser(sessionId) {
    const connection = this.connectedBrowsers.get(sessionId);
    if (connection) {
      try {
        connection.ws.close();
      } catch (error) {
        console.log(`🔌 DevTools: Error closing browser connection:`, error.message);
      }
      this.connectedBrowsers.delete(sessionId);
    }
  }

  /**
   * Get browser monitoring status for all clients
   */
  getClientStatus() {
    const clients = [];
    
    for (const [sessionId, client] of this.continuum.connectedClients.entries()) {
      clients.push({
        sessionId,
        browserDetected: client.browserDetected,
        browserType: client.browserType,
        devToolsPort: client.devToolsPort,
        consoleLogsEnabled: client.consoleLogsEnabled,
        connectedAt: client.connectedAt,
        isConnected: this.connectedBrowsers.has(sessionId)
      });
    }
    
    return {
      totalClients: clients.length,
      browsersConnected: clients.filter(c => c.isConnected).length,
      clients
    };
  }

  /**
   * Detect browser type from port
   */
  detectBrowserType(port) {
    switch (port) {
      case 9222: return 'Chrome/Opera';
      case 9223: return 'Edge';
      case 9224: return 'Safari';
      default: return 'Unknown';
    }
  }

  /**
   * Force rescan for browsers
   */
  async rescanBrowsers() {
    console.log('🔌 DevTools: Rescanning for browsers...');
    
    for (const sessionId of this.continuum.connectedClients.keys()) {
      await this.detectAndConnectBrowser(sessionId);
    }
  }
}

module.exports = BrowserDevToolsManager;