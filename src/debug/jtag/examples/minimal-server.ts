#!/usr/bin/env npx tsx
/**
 * Minimal Demo Server - Bypass JTAG init issues
 */

import * as http from 'http';
import * as path from 'path';
import * as fs from 'fs';

const PORT = 9002;

class MinimalServer {
  private server: http.Server;

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

  private handleAPIRoute(req: http.IncomingMessage, res: http.ServerResponse): void {
    // Mock API route response for browser client
    res.writeHead(200, { 
      'Content-Type': 'application/json',
      'Access-Control-Allow-Origin': '*',
      'Access-Control-Allow-Methods': 'POST, OPTIONS',
      'Access-Control-Allow-Headers': 'Content-Type'
    });
    
    if (req.method === 'OPTIONS') {
      res.end();
      return;
    }

    // Mock successful response
    res.end(JSON.stringify({ 
      success: true, 
      result: { message: 'Mock API response' }
    }));
    console.log(`🔄 Mock API route handled`);
  }

  private serve404(res: http.ServerResponse): void {
    res.writeHead(404, { 'Content-Type': 'text/html' });
    res.end('<h1>404 - Not Found</h1>');
  }

  async start(): Promise<void> {
    console.log('🚀 Starting minimal demo server...');
    
    return new Promise((resolve, reject) => {
      this.server.on('error', reject);
      this.server.listen(PORT, () => {
        console.log(`✅ Minimal server running at http://localhost:${PORT}`);
        
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