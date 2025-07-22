#!/usr/bin/env npx tsx
/**
 * Real JTAG Demo Server - Connect to actual JTAG system
 */

import * as http from 'http';
import * as path from 'path';
import * as fs from 'fs';
// Import symmetric daemon system
import { createJTAGDaemonSystem, JTAGRouter, DaemonMessage } from '../daemons/index';
// Import WebSocket server to start REAL JTAG on port 9001
import { JTAGWebSocketServer } from '../shared/JTAGWebSocket';

const PORT = 9002;

class MinimalServer {
  private server: http.Server;
  private jtagServer: JTAGWebSocketServer | null = null;
  private daemonSystem: any = null;

  constructor() {
    this.server = http.createServer(this.handleRequest.bind(this));
  }

  private handleRequest(req: http.IncomingMessage, res: http.ServerResponse): void {
    const url = req.url || '/';
    console.log(`📥 ${req.method} ${url}`);

    // Static file serving
    if (url === '/') {
      this.serveFile(res, 'demo.html', 'text/html');
    } else if (url === '/demo.css') {
      this.serveFile(res, 'demo.css', 'text/css');
    } else if (url === '/dist/demo.js') {
      this.serveFile(res, 'dist/examples/demo.js', 'application/javascript');
    } else if (url === '/browser-client/jtag-auto-init' || url === '/browser-client/jtag-auto-init.js') {
      this.serveFile(res, 'dist/jtag-auto-init.js', 'application/javascript');
    } else if (url === '/api/route') {
      this.handleAPIRoute(req, res);
    } else {
      this.serve404(res);
    }
  }

  private serveFile(res: http.ServerResponse, filename: string, contentType: string): void {
    const filePath = path.join(__dirname, filename);
    
    try {
      const content = fs.readFileSync(filePath, 'utf8');
      res.writeHead(200, { 
        'Content-Type': contentType,
        'Cache-Control': 'no-cache'
      });
      res.end(content);
      console.log(`✅ Served ${filename}`);
    } catch (error: any) {
      console.error(`❌ Failed to serve ${filename}:`, error.message);
      this.serve404(res);
    }
  }

  private async handleAPIRoute(req: http.IncomingMessage, res: http.ServerResponse): Promise<void> {
    // Real JTAG transport routing
    res.setHeader('Access-Control-Allow-Origin', '*');
    res.setHeader('Access-Control-Allow-Methods', 'POST, OPTIONS');
    res.setHeader('Access-Control-Allow-Headers', 'Content-Type');
    
    if (req.method === 'OPTIONS') {
      res.writeHead(200);
      res.end();
      return;
    }

    if (req.method !== 'POST') {
      res.writeHead(405, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ success: false, error: 'Method not allowed' }));
      return;
    }

    // Read request body
    let body = '';
    req.on('data', chunk => body += chunk);
    req.on('end', async () => {
      try {
        const message = JSON.parse(body);
        console.log(`🔄 Symmetric daemon routing message:`, message.type);
        
        // Route through symmetric daemon system
        const daemonMessage: DaemonMessage = {
          type: message.type,
          payload: message.payload || message,
          target: message.target
        };
        
        const results = await this.daemonSystem.sendMessage(daemonMessage);
        
        // Find successful result
        const successResult = results.find(r => r.success);
        
        res.writeHead(200, { 'Content-Type': 'application/json' });
        if (successResult?.result) {
          res.end(JSON.stringify({ success: true, result: successResult.result }));
          console.log(`✅ Symmetric daemon success: ${message.type}`);
        } else {
          // Return error details from failed results
          const errors = results.filter(r => !r.success).map(r => r.error);
          res.end(JSON.stringify({ success: false, error: errors }));
          console.log(`❌ Symmetric daemon failed: ${message.type}`, errors);
        }
      } catch (error: any) {
        console.error(`💥 JTAG route error:`, error.message);
        res.writeHead(500, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ success: false, error: error.message }));
      }
    });
  }

  private serve404(res: http.ServerResponse): void {
    res.writeHead(404, { 'Content-Type': 'text/html' });
    res.end('<h1>404 - Not Found</h1>');
  }

  async start(): Promise<void> {
    console.log('🚀 Starting minimal demo server...');
    
    // Initialize symmetric daemon system
    console.log('🚀 Initializing symmetric daemon system...');
    this.daemonSystem = createJTAGDaemonSystem('universal');
    await this.daemonSystem.registerDaemons();
    
    console.log('✅ Symmetric daemon system ready:');
    console.log(`   - Registered endpoints: ${this.daemonSystem.getRegisteredEndpoints().join(', ')}`);
    
    // START THE REAL WEBSOCKET SERVER ON PORT 9001
    console.log('🚀 Starting REAL JTAG WebSocket server on port 9001...');
    this.jtagServer = new JTAGWebSocketServer({
      port: 9001,
      onLog: (entry) => {
        console.log(`📝 JTAG Server received log:`, entry);
      },
      onScreenshot: async (payload) => {
        console.log(`📷 JTAG Server received screenshot:`, payload.filename);
        return { success: true, filename: payload.filename };
      },
      onExec: async (code, options) => {
        console.log(`⚡ JTAG Server received exec:`, code);
        return { success: true, result: 'Server exec result' };
      }
    });
    
    await this.jtagServer.start();
    console.log('✅ JTAG WebSocket server listening on port 9001');
    
    return new Promise((resolve, reject) => {
      this.server.on('error', reject);
      this.server.listen(PORT, () => {
        console.log(`✅ HTTP demo server running at http://localhost:${PORT}`);
        
        // Launch browser automatically
        setTimeout(() => {
          const { exec } = require('child_process');
          exec(`open http://localhost:${PORT}`, (error: any) => {
            if (error) {
              console.log('   ⚠️ Could not auto-launch browser:', error.message);
              console.log(`   👉 Manually open: http://localhost:${PORT}`);
            } else {
              console.log('   🚀 Browser launched automatically');
            }
          });
        }, 1000);
        
        resolve();
      });
    });
  }
}

// Start server
const server = new MinimalServer();
server.start().catch(console.error);