/**
 * DevTools Server
 * Universal HTTP/WebSocket server for DevTools capabilities
 * Can be used from Python, CLI, web interfaces, or any HTTP client
 */

const express = require('express');
const { createServer } = require('http');
const WebSocket = require('ws');
const DevToolsCore = require('./DevToolsCore.cjs');

class DevToolsServer {
  constructor(options = {}) {
    this.port = options.port || 9001;
    this.host = options.host || 'localhost';
    this.devtools = new DevToolsCore(options);
    this.app = express();
    this.server = null;
    this.wss = null;
    this.clients = new Set();
    
    this.setupExpress();
    this.setupWebSocket();
    this.setupDevToolsEvents();
  }

  setupExpress() {
    this.app.use(express.json());
    this.app.use(express.static('public')); // For web interface
    
    // CORS for cross-origin requests
    this.app.use((req, res, next) => {
      res.header('Access-Control-Allow-Origin', '*');
      res.header('Access-Control-Allow-Methods', 'GET, POST, PUT, DELETE, OPTIONS');
      res.header('Access-Control-Allow-Headers', 'Content-Type, Authorization');
      if (req.method === 'OPTIONS') {
        res.sendStatus(200);
      } else {
        next();
      }
    });

    // REST API endpoints
    this.setupRESTAPI();
  }

  setupRESTAPI() {
    // Status endpoint
    this.app.get('/api/status', (req, res) => {
      res.json({
        success: true,
        status: this.devtools.getStatus(),
        server: {
          port: this.port,
          clients: this.clients.size,
          uptime: process.uptime()
        }
      });
    });

    // Connect endpoint
    this.app.post('/api/connect', async (req, res) => {
      try {
        const { adapter = 'auto', ...options } = req.body;
        const success = await this.devtools.connect(adapter);
        
        res.json({
          success,
          message: success ? `Connected via ${adapter}` : 'Connection failed',
          status: this.devtools.getStatus()
        });
      } catch (error) {
        res.status(500).json({
          success: false,
          error: error.message
        });
      }
    });

    // Disconnect endpoint
    this.app.post('/api/disconnect', async (req, res) => {
      try {
        await this.devtools.disconnect();
        res.json({
          success: true,
          message: 'Disconnected'
        });
      } catch (error) {
        res.status(500).json({
          success: false,
          error: error.message
        });
      }
    });

    // Screenshot endpoint
    this.app.post('/api/screenshot', async (req, res) => {
      try {
        const { 
          format = 'png', 
          quality = 90, 
          fullPage = false,
          filename = null 
        } = req.body;

        const result = await this.takeScreenshot({
          format, quality, fullPage, filename
        });

        if (result.success && !filename) {
          // Return image data directly
          const buffer = Buffer.from(result.data, 'base64');
          res.set('Content-Type', `image/${format}`);
          res.send(buffer);
        } else {
          res.json(result);
        }
      } catch (error) {
        res.status(500).json({
          success: false,
          error: error.message
        });
      }
    });

    // Execute script endpoint
    this.app.post('/api/execute', async (req, res) => {
      try {
        const { script } = req.body;
        
        if (!script) {
          return res.status(400).json({
            success: false,
            error: 'Script parameter required'
          });
        }

        const result = await this.devtools.send('Runtime.evaluate', {
          expression: script,
          returnByValue: true,
          includeCommandLineAPI: true
        });

        res.json({
          success: true,
          result: result.result?.value,
          type: result.result?.type,
          error: result.exceptionDetails?.exception?.description
        });
      } catch (error) {
        res.status(500).json({
          success: false,
          error: error.message
        });
      }
    });

    // Console logs endpoint
    this.app.get('/api/console', (req, res) => {
      const { limit = 50, level = 'all' } = req.query;
      
      res.json({
        success: true,
        logs: this.getConsoleLogs(parseInt(limit), level),
        filters: { limit: parseInt(limit), level }
      });
    });

    // WebSocket frames endpoint
    this.app.get('/api/websocket', (req, res) => {
      const { limit = 100, direction = 'all' } = req.query;
      
      res.json({
        success: true,
        frames: this.getWebSocketFrames(parseInt(limit), direction),
        filters: { limit: parseInt(limit), direction }
      });
    });

    // Generic command endpoint (for Continuum integration)
    this.app.post('/api/command', async (req, res) => {
      try {
        const { action, params = {} } = req.body;
        const result = await this.executeCommand(action, params);
        res.json(result);
      } catch (error) {
        res.status(500).json({
          success: false,
          error: error.message
        });
      }
    });
  }

  setupWebSocket() {
    this.server = createServer(this.app);
    this.wss = new WebSocket.Server({ server: this.server });

    this.wss.on('connection', (ws) => {
      console.log('🔌 DevTools: WebSocket client connected');
      this.clients.add(ws);

      ws.on('message', async (data) => {
        try {
          const message = JSON.parse(data);
          const result = await this.handleWebSocketMessage(message);
          ws.send(JSON.stringify({ id: message.id, ...result }));
        } catch (error) {
          ws.send(JSON.stringify({
            id: message.id || null,
            success: false,
            error: error.message
          }));
        }
      });

      ws.on('close', () => {
        console.log('🔌 DevTools: WebSocket client disconnected');
        this.clients.delete(ws);
      });

      // Send initial status
      ws.send(JSON.stringify({
        type: 'status',
        status: this.devtools.getStatus()
      }));
    });
  }

  async handleWebSocketMessage(message) {
    const { action, params = {} } = message;
    return await this.executeCommand(action, params);
  }

  setupDevToolsEvents() {
    // Forward DevTools events to WebSocket clients
    this.devtools.on('data', (data) => {
      this.broadcast({
        type: 'devtools-data',
        data
      });
    });

    this.devtools.on('connected', (adapter) => {
      this.broadcast({
        type: 'devtools-connected',
        adapter
      });
    });

    this.devtools.on('disconnected', () => {
      this.broadcast({
        type: 'devtools-disconnected'
      });
    });
  }

  broadcast(message) {
    const data = JSON.stringify(message);
    this.clients.forEach(client => {
      if (client.readyState === WebSocket.OPEN) {
        client.send(data);
      }
    });
  }

  async executeCommand(action, params) {
    switch (action) {
      case 'connect':
        return await this.connect(params);
      case 'disconnect':
        return await this.disconnect();
      case 'status':
        return await this.getStatus();
      case 'screenshot':
        return await this.takeScreenshot(params);
      case 'execute':
        return await this.executeScript(params);
      case 'console':
        return await this.getConsole(params);
      case 'websocket':
        return await this.getWebSocket(params);
      default:
        return {
          success: false,
          error: `Unknown action: ${action}`,
          available_actions: [
            'connect', 'disconnect', 'status', 'screenshot', 
            'execute', 'console', 'websocket'
          ]
        };
    }
  }

  // Command implementations
  async connect(params) {
    const { adapter = 'auto' } = params;
    const success = await this.devtools.connect(adapter);
    return {
      success,
      message: success ? `Connected via ${adapter}` : 'Connection failed',
      status: this.devtools.getStatus()
    };
  }

  async disconnect() {
    await this.devtools.disconnect();
    return {
      success: true,
      message: 'Disconnected'
    };
  }

  async getStatus() {
    return {
      success: true,
      status: this.devtools.getStatus()
    };
  }

  async takeScreenshot(params) {
    // Implementation similar to DevToolsCommand
    if (!this.devtools.activeAdapter) {
      return {
        success: false,
        error: 'No active DevTools connection'
      };
    }

    const adapter = this.devtools.activeAdapter.instance;
    if (!adapter.takeScreenshot) {
      return {
        success: false,
        error: 'Screenshot not supported by current adapter'
      };
    }

    return await adapter.takeScreenshot(params);
  }

  async executeScript(params) {
    const { script } = params;
    return await this.devtools.send('Runtime.evaluate', {
      expression: script,
      returnByValue: true
    });
  }

  async getConsole(params) {
    const { limit = 50, level = 'all' } = params;
    return {
      success: true,
      logs: this.getConsoleLogs(limit, level)
    };
  }

  async getWebSocket(params) {
    const { limit = 100, direction = 'all' } = params;
    return {
      success: true,
      frames: this.getWebSocketFrames(limit, direction)
    };
  }

  // Data storage methods (would need to implement history storage)
  getConsoleLogs(limit, level) {
    // Return stored console logs
    return [];
  }

  getWebSocketFrames(limit, direction) {
    // Return stored WebSocket frames
    return [];
  }

  async start() {
    return new Promise((resolve, reject) => {
      this.server.listen(this.port, this.host, (error) => {
        if (error) {
          reject(error);
        } else {
          console.log(`🔌 DevTools Server: Running on http://${this.host}:${this.port}`);
          console.log(`🔌 WebSocket: ws://${this.host}:${this.port}`);
          console.log(`🔌 REST API: http://${this.host}:${this.port}/api/`);
          resolve();
        }
      });
    });
  }

  async stop() {
    if (this.server) {
      await this.devtools.disconnect();
      this.server.close();
      console.log('🔌 DevTools Server: Stopped');
    }
  }
}

module.exports = DevToolsServer;