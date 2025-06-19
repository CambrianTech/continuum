/**
 * DevTools Connection Handler
 * Manages connection to browser DevTools API
 */

const WebSocket = require('ws');

class DevToolsConnector {
  constructor() {
    this.connections = new Map(); // sessionId -> connection
  }

  async tryConnect(port) {
    try {
      const response = await this.fetchTargets(port);
      const targets = await response.json();
      
      // Look for Continuum page
      const continuumTarget = targets.find(target => 
        target.type === 'page' && 
        (target.url.includes('localhost:9000') || 
         target.title.toLowerCase().includes('continuum'))
      );

      return continuumTarget ? { target: continuumTarget, targets } : null;
    } catch (error) {
      return null;
    }
  }

  async connect(sessionId, target) {
    try {
      const ws = new WebSocket(target.webSocketDebuggerUrl);
      
      await new Promise((resolve, reject) => {
        ws.on('open', resolve);
        ws.on('error', reject);
      });

      // Enable console logging
      ws.send(JSON.stringify({ id: 1, method: 'Log.enable', params: {} }));
      ws.send(JSON.stringify({ id: 2, method: 'Runtime.enable', params: {} }));

      this.connections.set(sessionId, { ws, target });
      
      return ws;
    } catch (error) {
      throw new Error(`Failed to connect to DevTools: ${error.message}`);
    }
  }

  disconnect(sessionId) {
    const connection = this.connections.get(sessionId);
    if (connection) {
      connection.ws.close();
      this.connections.delete(sessionId);
    }
  }

  fetchTargets(port) {
    // Simple fetch implementation without external deps
    const https = require('https');
    const http = require('http');
    
    return new Promise((resolve, reject) => {
      const protocol = port === 443 ? https : http;
      const req = protocol.get(`http://localhost:${port}/json`, { timeout: 2000 }, (res) => {
        let data = '';
        res.on('data', chunk => data += chunk);
        res.on('end', () => {
          resolve({
            json: () => Promise.resolve(JSON.parse(data)),
            ok: res.statusCode >= 200 && res.statusCode < 300
          });
        });
      });
      req.on('error', reject);
      req.on('timeout', () => reject(new Error('Request timeout')));
    });
  }
}

module.exports = DevToolsConnector;