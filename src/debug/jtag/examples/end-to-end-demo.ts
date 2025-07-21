#!/usr/bin/env npx tsx
/**
 * JTAG End-to-End Demo Server
 * 
 * Serves the JTAG demo application on port 9002
 * Demonstrates both server-side and browser-side JTAG functionality
 */

import * as http from 'http';
import * as path from 'path';
import * as fs from 'fs';
import { jtag, jtagRouter } from '../index';

const PORT = 9002;
const JTAG_PORT = 9001;

class JTAGDemoServer {
  private server: http.Server;
  private serverUUID: string;

  constructor() {
    this.server = http.createServer(this.handleRequest.bind(this));
    this.serverUUID = jtag.getUUID().uuid;
  }

  private handleRequest(req: http.IncomingMessage, res: http.ServerResponse): void {
    const url = req.url || '/';
    
    jtag.log('DEMO_REQUEST', `${req.method} ${url}`, {
      userAgent: req.headers['user-agent']?.substring(0, 50) + '...',
      timestamp: new Date().toISOString()
    });

    // Static Files - Route through JTAG router instead of serving directly
    const staticFiles = ['/', '/demo.css', '/demo.js', '/jtag.js'];
    if (staticFiles.includes(url)) {
      this.serveViaRouter(url, res);
      return;
    }

    // JTAG Route Namespace - /jtag/* (after specific file checks)
    if (url.startsWith('/jtag')) {
      this.handleJTAGRoute(url, req, res);
      return;
    }

    // API Routes
    if (url.startsWith('/api/')) {
      this.handleAPIRoute(url, req, res);
      return;
    }

    // 404 for everything else
    this.serve404(res);
  }

  private handleJTAGRoute(url: string, req: http.IncomingMessage, res: http.ServerResponse): void {
    // JTAG owns all /jtag/* routes
    switch (url) {
      case '/jtag':
      case '/jtag/':
        // JTAG status endpoint
        res.writeHead(200, { 
          'Content-Type': 'application/json',
          'Access-Control-Allow-Origin': '*'
        });
        res.end(JSON.stringify({
          status: 'ready',
          system: 'JTAG Universal Debugging',
          version: '1.0.1',
          transport: 'websocket',
          endpoint: `ws://localhost:${JTAG_PORT}`
        }));
        break;

      case '/jtag.js':
        // Serve JTAG browser client
        this.serveJTAGScript(res);
        break;

      case '/jtag/health':
        // JTAG health check
        res.writeHead(200, { 
          'Content-Type': 'application/json',
          'Access-Control-Allow-Origin': '*'
        });
        res.end(JSON.stringify({
          healthy: true,
          uuid: this.serverUUID,
          uptime: process.uptime(),
          timestamp: new Date().toISOString()
        }));
        break;

      default:
        jtag.log('DEMO_404', `JTAG route not found: ${url}`);
        this.serve404(res);
    }
  }

  private handleAPIRoute(url: string, req: http.IncomingMessage, res: http.ServerResponse): void {
    // Non-JTAG API routes
    switch (url) {
      case '/api/server-info':
        this.serveServerInfo(res);
        break;

      case '/api/exec':
        this.handleServerExec(res);
        break;

      default:
        this.serve404(res);
    }
  }

  async start(): Promise<void> {
    // Initialize JTAG system first
    console.log('🎪 Starting JTAG End-to-End Demo Server...');
    
    // CRITICAL: Connect with health check BEFORE starting HTTP server
    console.log('🔗 Testing JTAG transport layer...');
    
    try {
      const connectionResult = await jtag.connect({ 
        healthCheck: true,
        transport: 'auto',
        timeout: 5000
      });
      
      console.log('✅ JTAG transport layer verified:', {
        transport: connectionResult.transport,
        latency: connectionResult.latency + 'ms',
        connected: connectionResult.connected
      });
      
    } catch (error: any) {
      console.error('❌ JTAG transport layer failed:', error.message);
      throw new Error('Cannot start demo server - JTAG transport not working');
    }
    
    jtag.log('DEMO_SERVER', 'Initializing JTAG demo application', {
      port: PORT,
      jtagPort: JTAG_PORT,
      serverUUID: this.serverUUID
    });

    return new Promise((resolve, reject) => {
      this.server.on('error', (error: Error) => {
        console.error('❌ Demo server error:', error.message);
        reject(error);
      });

      this.server.listen(PORT, () => {
        console.log(`✅ Demo server running at http://localhost:${PORT}`);
        console.log(`📝 JTAG server running on port ${JTAG_PORT}`);
        console.log(`🆔 Server UUID: ${this.serverUUID}`);
        
        jtag.log('DEMO_SERVER', 'Demo server started successfully', {
          url: `http://localhost:${PORT}`,
          jtagUrl: `ws://localhost:${JTAG_PORT}`
        });

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

  private async waitForJTAGReady(): Promise<void> {
    // Wait for JTAG to be fully initialized
    return new Promise((resolve) => {
      if (jtag.isInitialized()) {
        resolve();
        return;
      }
      
      // Wait up to 5 seconds for JTAG to be ready
      const checkReady = () => {
        if (jtag.isInitialized()) {
          resolve();
        } else {
          setTimeout(checkReady, 100);
        }
      };
      
      checkReady();
    });
  }

  private async serveViaRouter(url: string, res: http.ServerResponse): Promise<void> {
    // Map URL to filename
    let filename = url === '/' ? 'demo.html' : url.substring(1);
    
    // Special handling for jtag.js - it's in parent directory
    if (filename === 'jtag.js') {
      filename = '../jtag.js';
    }

    try {
      // Create simple static-file message adhering to JTAGUniversalMessage
      const message = {
        id: 'static_' + Math.random().toString(36).substr(2, 9),
        type: 'static-file' as const,
        version: '1.0.0',
        timestamp: new Date().toISOString(),
        source: 'server' as const,
        payload: {
          filename,
          path: url,
          method: 'GET'
        }
      };

      console.log(`🔄 Demo Server: Routing ${url} through JTAG router`);
      
      // Route through JTAG router
      const results = await jtagRouter.routeMessage(message);
      
      // Find successful result
      const successResult = results.find(r => r.success);
      
      if (successResult?.result) {
        const fileData = successResult.result;
        
        res.writeHead(fileData.status || 200, fileData.headers || {});
        res.end(fileData.content);
        
        jtag.log('DEMO_ROUTER_STATIC', `Served ${filename} via router`, {
          transport: fileData.transport,
          size: fileData.size,
          routeResults: results.length
        });
        
      } else {
        // Router failed - fall back to 404
        const errorResults = results.filter(r => !r.success);
        jtag.error('DEMO_ROUTER_STATIC', `Router failed to serve ${filename}`, {
          errors: errorResults.map(r => r.error)
        });
        
        this.serve404(res);
      }
      
    } catch (error: any) {
      jtag.error('DEMO_ROUTER_STATIC', `Router error serving ${filename}`, {
        error: error.message,
        url
      });
      
      this.serve404(res);
    }
  }

  private serveStaticFile(res: http.ServerResponse, filename: string, contentType: string): void {
    const filePath = path.join(__dirname, filename);
    
    try {
      const content = fs.readFileSync(filePath, 'utf8');
      
      res.writeHead(200, { 
        'Content-Type': contentType,
        'Cache-Control': 'no-cache'
      });
      res.end(content);
      
      jtag.log('DEMO_STATIC', `Served ${filename}`, { 
        size: content.length,
        type: contentType 
      });
      
    } catch (error: any) {
      jtag.error('DEMO_STATIC', `Failed to serve ${filename}`, { 
        error: error.message,
        path: filePath 
      });
      
      this.serve404(res);
    }
  }

  private serveJTAGScript(res: http.ServerResponse): void {
    const jtagScriptPath = path.join(__dirname, '../jtag.js');
    
    try {
      const content = fs.readFileSync(jtagScriptPath, 'utf8');
      
      res.writeHead(200, { 
        'Content-Type': 'application/javascript',
        'Cache-Control': 'no-cache',
        'Access-Control-Allow-Origin': '*'
      });
      res.end(content);
      
      jtag.log('DEMO_STATIC', 'Served jtag.js', { 
        size: content.length,
        path: jtagScriptPath 
      });
      
    } catch (error: any) {
      jtag.error('DEMO_STATIC', 'Failed to serve jtag.js', { 
        error: error.message,
        path: jtagScriptPath 
      });
      
      this.serve404(res);
    }
  }

  private serveServerInfo(res: http.ServerResponse): void {
    const serverInfo = {
      uuid: this.serverUUID,
      uptime: process.uptime() * 1000,
      jtagPort: JTAG_PORT,
      demoPort: PORT,
      timestamp: new Date().toISOString()
    };

    jtag.log('DEMO_API', 'Server info requested', serverInfo);

    res.writeHead(200, { 
      'Content-Type': 'application/json',
      'Access-Control-Allow-Origin': '*'
    });
    res.end(JSON.stringify(serverInfo));
  }

  private async handleServerExec(res: http.ServerResponse): Promise<void> {
    try {
      const execResult = await jtag.exec('process.version + " running for " + Math.floor(process.uptime()) + " seconds"');
      
      jtag.log('DEMO_EXEC', 'Server-side exec test', {
        code: 'process.version + uptime',
        result: execResult.result,
        executionTime: execResult.executionTime
      });

      res.writeHead(200, { 
        'Content-Type': 'application/json',
        'Access-Control-Allow-Origin': '*'
      });
      res.end(JSON.stringify(execResult));
    } catch (error: any) {
      jtag.error('DEMO_EXEC', 'Server exec failed', { error: error.message });
      
      res.writeHead(500, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ success: false, error: error.message }));
    }
  }

  private serveHealth(res: http.ServerResponse): void {
    res.writeHead(200, { 
      'Content-Type': 'application/json',
      'Access-Control-Allow-Origin': '*' 
    });
    res.end(JSON.stringify({ 
      status: 'ok', 
      uuid: this.serverUUID,
      timestamp: new Date().toISOString() 
    }));
  }

  private serve404(res: http.ServerResponse): void {
    jtag.log('DEMO_404', '404 Not Found', { url: res.req?.url });
    
    res.writeHead(404, { 'Content-Type': 'text/html' });
    res.end(`
      <h1>404 - Not Found</h1>
      <p>Go back to <a href="/">JTAG Demo</a></p>
    `);
  }

  private async waitForJTAGReady(): Promise<void> {
    // Simple delay to allow JTAG WebSocket to initialize
    return new Promise(resolve => setTimeout(resolve, 1000));
  }

  stop(): void {
    this.server.close();
    jtag.log('DEMO_SERVER', 'Demo server stopped');
  }
}

// Start the demo server if called directly
if (require.main === module) {
  const demo = new JTAGDemoServer();
  demo.start().catch(error => {
    console.error('Failed to start demo server:', error);
    process.exit(1);
  });

  // Graceful shutdown
  process.on('SIGINT', () => {
    console.log('\n🛑 Shutting down demo server...');
    demo.stop();
    process.exit(0);
  });
}

export default JTAGDemoServer;