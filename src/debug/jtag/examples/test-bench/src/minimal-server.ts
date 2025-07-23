#!/usr/bin/env npx tsx
/**
 * Real JTAG Demo Server - Connect to actual JTAG system
 */

import * as http from 'http';
import * as path from 'path';
import * as fs from 'fs';
// Import unified JTAG system from npm module
import { jtag, JTAGSystem } from '@continuum/jtag';

// Read port from package.json config
const packageJson = require('../package.json');
const PORT = packageJson.config?.port || 9002;

class MinimalServer {
  private server: http.Server;
  // Removed unused jtagServer reference
  private requestInProgress = false;

  constructor() {
    this.server = http.createServer(this.handleRequest.bind(this));
  }

  private handleRequest(req: http.IncomingMessage, res: http.ServerResponse): void {
    // Prevent recursion in request handling
    if (this.requestInProgress) {
      res.writeHead(503, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ error: 'Server busy - recursion detected' }));
      return;
    }
    
    this.requestInProgress = true;
    
    try {
      const url = req.url || '/';
      const method = req.method || 'GET';
      
      // Comprehensive request logging with error handling
      try {
        console.log(`📥 ${method} ${url}`);
      } catch (logError) {
        // Fallback logging if main console fails
        process.stderr.write(`[LOG ERROR] ${method} ${url} - Logging failed: ${logError}\n`);
      }

      // Static file serving with comprehensive error handling
      if (url === '/') {
        this.serveFile(res, '../public/demo.html', 'text/html');
      } else if (url === '/demo.css') {
        this.serveFile(res, '../public/demo.css', 'text/css');
      } else if (url.startsWith('/dist/')) {
        // Serve any file from dist directory
        const filePath = '..' + url; // Remove the extra slash
        const contentType = url.endsWith('.js') ? 'application/javascript' : 
                           url.endsWith('.css') ? 'text/css' :
                           url.endsWith('.json') ? 'application/json' : 'application/octet-stream';
        console.log(`🔍 Serving dist file: ${url} -> ${filePath} (${contentType})`);
        this.serveFile(res, filePath, contentType);
      } else if (url === '/favicon.ico') {
        // Ignore favicon requests
        this.serve404(res);
      } else if (url === '/api/route') {
        // HTTP fallback removed - WebSocket transport only
        res.writeHead(410, { 'Content-Type': 'application/json' });
        res.end(JSON.stringify({ 
          success: false, 
          error: 'HTTP /api/route deprecated - use WebSocket transport on port 9001' 
        }));
      } else {
        this.serve404(res);
      }
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.error('🚨 Minimal Server: Request handling failed:', errorMsg);
      
      try {
        if (!res.headersSent) {
          res.writeHead(500, { 'Content-Type': 'application/json' });
          res.end(JSON.stringify({ error: 'Internal server error', details: errorMsg }));
        }
      } catch (responseError) {
        process.stderr.write(`[RESPONSE ERROR] Could not send error response: ${responseError}\n`);
      }
    } finally {
      this.requestInProgress = false;
    }
  }

  private serveFile(res: http.ServerResponse, filename: string, contentType: string): void {
    let filePath = '';
    
    try {
      filePath = path.join(__dirname, filename);
      
      // Check if file exists before trying to read
      if (!fs.existsSync(filePath)) {
        console.error(`❌ File not found: ${filePath}`);
        console.error(`❌ __dirname: ${__dirname}`);
        console.error(`❌ filename: ${filename}`);
        this.serve404(res);
        return;
      }
      
      const content = fs.readFileSync(filePath, 'utf8');
      
      if (!res.headersSent) {
        res.writeHead(200, { 
          'Content-Type': contentType,
          'Cache-Control': 'no-cache, no-store, must-revalidate',
          'Pragma': 'no-cache',
          'Expires': '0',
          'Content-Length': Buffer.byteLength(content, 'utf8')
        });
      }
      
      res.end(content);
      console.log(`✅ Served ${filename} (${Buffer.byteLength(content, 'utf8')} bytes)`);
      
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      console.error(`❌ Failed to serve ${filename} from ${filePath}:`, errorMsg);
      
      try {
        this.serve404(res);
      } catch (fallbackError) {
        process.stderr.write(`[FALLBACK ERROR] Could not serve 404: ${fallbackError}\n`);
        if (!res.headersSent) {
          res.writeHead(500);
          res.end('Server Error');
        }
      }
    }
  }


  private serve404(res: http.ServerResponse): void {
    res.writeHead(404, { 'Content-Type': 'text/html' });
    res.end('<h1>404 - Not Found</h1>');
  }

  async start(): Promise<void> {
    console.log('🚀 Starting test-bench demo server...');
    
    // Use the unified JTAG system
    console.log('📊 JTAG Environment:', jtag.environment);
    console.log('📊 JTAG Context:', jtag.context);
    
    // Test console logging to verify ConsoleDaemon works
    console.log('🧪 Testing console logging through ConsoleDaemon...');
    console.warn('This is a test warning from test-bench');
    console.error('This is a test error from test-bench');
    
    return new Promise((resolve, reject) => {
      this.server.on('error', reject);
      this.server.listen(PORT, () => {
        console.log(`✅ HTTP demo server running at http://localhost:${PORT}`);
        
        // Launch browser automatically with error handling
        setTimeout(() => {
          try {
            const { exec } = require('child_process');
            exec(`open http://localhost:${PORT}`, (error: any) => {
              if (error) {
                const errorMsg = error instanceof Error ? error.message : String(error);
                console.log('   ⚠️ Could not auto-launch browser:', errorMsg);
                console.log(`   👉 Manually open: http://localhost:${PORT}`);
              } else {
                console.log('   🚀 Browser launched automatically');
              }
            });
          } catch (launchError) {
            const errorMsg = launchError instanceof Error ? launchError.message : String(launchError);
            console.error('   ❌ Browser launch setup failed:', errorMsg);
            console.log(`   👉 Manually open: http://localhost:${PORT}`);
          }
        }, 1000);
        
        resolve();
      });
    });
  }
}

// Start server with comprehensive error handling
const server = new MinimalServer();
server.start().catch((startupError) => {
  const errorMsg = startupError instanceof Error ? startupError.message : String(startupError);
  console.error('🚨 Minimal Server startup failed:', errorMsg);
  
  // Log full error details for debugging
  if (startupError instanceof Error && startupError.stack) {
    console.error('Stack trace:', startupError.stack);
  }
  
  process.exit(1);
});

// Global error handlers
process.on('uncaughtException', (error) => {
  console.error('🚨 Uncaught Exception:', error.message);
  if (error.stack) console.error('Stack:', error.stack);
  process.exit(1);
});

process.on('unhandledRejection', (reason, promise) => {
  console.error('🚨 Unhandled Rejection at:', promise);
  console.error('Reason:', reason);
  process.exit(1);
});