#!/usr/bin/env npx tsx
/**
 * Shared Minimal Server Template for Examples
 * Simple HTTP server that serves static files for JTAG examples
 */

import * as http from 'http';
import * as path from 'path';
import * as fs from 'fs';

// Read port from the example's package.json config  
const packageJson = require(process.cwd() + '/package.json');
const PORT = packageJson.config?.port || 9002;

class MinimalServer {
  private server: http.Server;
  private requestInProgress = false;

  constructor() {
    this.server = http.createServer(this.handleRequest.bind(this));
  }

  private handleRequest(req: http.IncomingMessage, res: http.ServerResponse): void {
    if (this.requestInProgress) {
      res.writeHead(503, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ error: 'Server busy' }));
      return;
    }
    
    this.requestInProgress = true;
    
    try {
      const url = req.url || '/';
      const method = req.method || 'GET';
      
      console.log(`📥 ${method} ${url}`);

      if (url === '/') {
        this.serveFile(res, 'public/demo.html', 'text/html');
      } else if (url === '/demo.css') {
        this.serveFile(res, 'public/demo.css', 'text/css');
      } else if (url.startsWith('/dist/')) {
        const filePath = 'dist' + url.substring(5);
        const contentType = url.endsWith('.js') ? 'application/javascript' : 
                           url.endsWith('.css') ? 'text/css' :
                           url.endsWith('.json') ? 'application/json' : 'application/octet-stream';
        this.serveFile(res, filePath, contentType);
      } else if (url === '/favicon.ico') {
        this.serve404(res);
      } else {
        this.serve404(res);
      }
    } catch (error) {
      console.error('🚨 Request handling failed:', error);
      try {
        if (!res.headersSent) {
          res.writeHead(500, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: 'Internal server error' }));
        }
      } catch (responseError) {
        console.error('Could not send error response:', responseError);
      }
    } finally {
      this.requestInProgress = false;
    }
  }

  private serveFile(res: http.ServerResponse, filename: string, contentType: string): void {
    try {
      // Use current working directory (the example directory) instead of __dirname (scripts directory)
      const filePath = path.join(process.cwd(), filename);
      
      if (!fs.existsSync(filePath)) {
        console.error(`❌ File not found: ${filePath}`);
        this.serve404(res);
        return;
      }
      
      const content = fs.readFileSync(filePath, 'utf8');
      
      if (!res.headersSent) {
        res.writeHead(200, { 
          'Content-Type': contentType,
          'Cache-Control': 'no-cache, no-store, must-revalidate',
          'Content-Length': Buffer.byteLength(content, 'utf8')
        });
      }
      
      res.end(content);
      
    } catch (error) {
      console.error(`❌ Failed to serve ${filename}:`, error);
      this.serve404(res);
    }
  }

  private serve404(res: http.ServerResponse): void {
    res.writeHead(404, { 'Content-Type': 'text/html' });
    res.end('<h1>404 - Not Found</h1>');
  }

  async start(): Promise<void> {
    const exampleName = path.basename(process.cwd());
    console.log(`🚀 Starting ${exampleName} HTTP server...`);
    console.log('🌐 Browser client will connect to JTAG system via WebSocket');
    
    return new Promise((resolve, reject) => {
      this.server.on('error', reject);
      this.server.listen(PORT, () => {
        console.log(`✅ HTTP server running at http://localhost:${PORT}`);
        
        // Launch browser automatically
        setTimeout(() => {
          try {
            const { exec } = require('child_process');
            const url = `http://localhost:${PORT}`;
            
            // Try multiple browser launch methods
            const browserCommands = [
              `open "${url}"`,                           // macOS default
              `xdg-open "${url}"`,                      // Linux default
              `start "" "${url}"`,                      // Windows default
              `google-chrome "${url}"`,                // Direct Chrome
              `firefox "${url}"`,                      // Direct Firefox
              `safari "${url}"`                        // Direct Safari
            ];
            
            let attempts = 0;
            const tryLaunch = () => {
              if (attempts >= browserCommands.length) {
                console.log('   ❌ CRITICAL: Could not auto-launch browser with any method');
                console.log(`   👉 MANUAL ACTION REQUIRED: Open ${url} in your browser`);
                console.log('   📋 This is a major failure - system needs browser interaction');
                return;
              }
              
              const command = browserCommands[attempts];
              console.log(`   🔍 Attempting browser launch method ${attempts + 1}: ${command}`);
              
              exec(command, { timeout: 5000 }, (error: any) => {
                if (error) {
                  console.log(`   ⚠️ Method ${attempts + 1} failed: ${error.message}`);
                  attempts++;
                  tryLaunch();
                } else {
                  console.log('   🚀 Browser launched automatically');
                  console.log(`   🌐 Opening: ${url}`);
                }
              });
            };
            
            tryLaunch();
            
          } catch (launchError) {
            console.log('   ❌ CRITICAL: Browser launch system failure');
            console.log(`   👉 MANUAL ACTION REQUIRED: Open http://localhost:${PORT} in your browser`);
            console.log('   📋 This is a major failure - system needs browser interaction');
          }
        }, 1000);
        
        resolve();
      });
    });
  }
}

// Start server
const server = new MinimalServer();
server.start().catch((error) => {
  console.error('🚨 Server startup failed:', error);
  process.exit(1);
});

// Global error handlers
process.on('uncaughtException', (error) => {
  console.error('🚨 Uncaught Exception:', error.message);
  process.exit(1);
});

process.on('unhandledRejection', (reason) => {
  console.error('🚨 Unhandled Rejection:', reason);
  process.exit(1);
});