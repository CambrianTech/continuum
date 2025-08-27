#!/usr/bin/env npx tsx
/**
 * JTAG Universal Middleware
 * 
 * Google Analytics-style integration for existing Node.js applications
 * 
 * Usage in any Express/Connect app:
 * 
 * ```javascript
 * const jtag = require('@continuum/jtag');
 * app.use(jtag.middleware());
 * ```
 * 
 * Or for automatic startup:
 * ```javascript
 * require('@continuum/jtag/auto-start');  // Just works!
 * ```
 */

import { Request, Response, NextFunction } from 'express';
import { JTAGSystemServer } from './system/core/system/server/JTAGSystemServer';

interface JTAGMiddlewareOptions {
  /** Enable automatic browser script injection */
  injectBrowserClient?: boolean;
  /** Custom path prefix for JTAG endpoints */
  pathPrefix?: string;
  /** Enable development features */
  development?: boolean;
  /** Custom WebSocket port (overrides examples.json) */
  websocketPort?: number;
}

class JTAGMiddleware {
  private system: any = null;
  private initialized = false;
  private options: Required<JTAGMiddlewareOptions>;

  constructor(options: JTAGMiddlewareOptions = {}) {
    this.options = {
      injectBrowserClient: true,
      pathPrefix: '/__jtag',
      development: process.env.NODE_ENV !== 'production',
      websocketPort: 0, // Will be set from config
      ...options
    };
  }

  /**
   * Initialize JTAG system (lazy initialization)
   */
  private async initialize(): Promise<void> {
    if (this.initialized) return;

    try {
      console.log('🚀 JTAG: Initializing universal debugging system...');
      
      // Start JTAG system - it will use context configuration internally
      this.system = await JTAGSystemServer.connect();
      this.initialized = true;
      
      // Get actual port from the system's context after initialization
      const wsPort = this.system.context?.config?.instance?.ports?.websocket_server || 
                    this.options.websocketPort || 9001;
      
      console.log(`✅ JTAG: System initialized on WebSocket port ${wsPort}`);
      console.log(`🌐 JTAG: Browser interface available at ${this.options.pathPrefix}/`);
      
    } catch (error) {
      console.error('❌ JTAG: Failed to initialize system:', error.message);
      // Don't throw - middleware should be non-breaking
    }
  }

  /**
   * Express/Connect middleware
   */
  middleware() {
    return async (req: Request, res: Response, next: NextFunction) => {
      try {
        // Lazy initialization
        await this.initialize();

        // Handle JTAG endpoints
        if (req.path.startsWith(this.options.pathPrefix)) {
          return this.handleJTAGRequest(req, res, next);
        }

        // Auto-inject browser client into HTML responses
        if (this.options.injectBrowserClient && this.shouldInjectClient(req, res)) {
          return this.injectBrowserClient(req, res, next);
        }

        next();
      } catch (error) {
        console.error('❌ JTAG: Middleware error:', error.message);
        // Never break the main application
        next();
      }
    };
  }

  /**
   * Handle JTAG-specific requests
   */
  private handleJTAGRequest(req: Request, res: Response, next: NextFunction): void {
    const jtagPath = req.path.substring(this.options.pathPrefix.length);

    switch (jtagPath) {
      case '/':
      case '/dashboard':
        this.serveDashboard(res);
        break;
      case '/browser-client.js':
        this.serveBrowserClient(res);
        break;
      case '/health':
        res.json({ status: 'healthy', initialized: this.initialized });
        break;
      default:
        next(); // Let other middleware handle it
    }
  }

  /**
   * Serve minimal dashboard HTML
   */
  private serveDashboard(res: Response): void {
    const wsPort = this.system?.context?.config?.instance?.ports?.websocket_server || 
                  this.options.websocketPort || 9001;
    const dashboardHTML = `
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>JTAG Debug Dashboard</title>
    <style>
        body { 
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
            background: #0f1419; color: #e0e6ed; margin: 0; padding: 20px;
        }
        .container { max-width: 800px; margin: 0 auto; }
        .status { padding: 10px; border-radius: 6px; margin: 10px 0; }
        .healthy { background: rgba(0, 212, 255, 0.1); border: 1px solid #00d4ff; }
        h1 { color: #00d4ff; }
        button { 
            background: #00d4ff; color: #000; border: none; padding: 10px 20px; 
            border-radius: 6px; cursor: pointer; margin: 5px;
        }
        button:hover { background: #00b8e6; }
        #output { 
            background: rgba(255,255,255,0.05); padding: 15px; border-radius: 6px;
            font-family: 'Monaco', 'Consolas', monospace; font-size: 12px;
            min-height: 200px; white-space: pre-wrap;
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>🚀 JTAG Universal Debug Dashboard</h1>
        
        <div class="status healthy">
            <strong>Status:</strong> System Active<br>
            <strong>WebSocket:</strong> ws://localhost:${wsPort}<br>
            <strong>Integration:</strong> Middleware Mode
        </div>
        
        <div>
            <button onclick="testScreenshot()">📸 Test Screenshot</button>
            <button onclick="testHealth()">🏥 Health Check</button>
            <button onclick="testCommand()">⚡ Test Command</button>
        </div>
        
        <div id="output">Welcome to JTAG Debug Dashboard\nSystem initialized and ready for debugging...</div>
    </div>
    
    <script src="${this.options.pathPrefix}/browser-client.js"></script>
    <script>
        async function testScreenshot() {
            log('📸 Testing screenshot...');
            try {
                const result = await jtag.commands.screenshot({ selector: 'body' });
                log('✅ Screenshot captured: ' + JSON.stringify(result, null, 2));
            } catch (error) {
                log('❌ Screenshot failed: ' + error.message);
            }
        }
        
        async function testHealth() {
            log('🏥 Checking system health...');
            try {
                const result = await jtag.commands.health();
                log('✅ Health check: ' + JSON.stringify(result, null, 2));
            } catch (error) {
                log('❌ Health check failed: ' + error.message);
            }
        }
        
        async function testCommand() {
            log('⚡ Listing available commands...');
            try {
                const result = await jtag.commands.list();
                log('✅ Available commands: ' + JSON.stringify(result, null, 2));
            } catch (error) {
                log('❌ Command listing failed: ' + error.message);
            }
        }
        
        function log(message) {
            const output = document.getElementById('output');
            output.textContent += '\\n' + new Date().toLocaleTimeString() + ': ' + message;
            output.scrollTop = output.scrollHeight;
        }
        
        // Initialize JTAG client
        jtag.connect().then(() => {
            log('✅ JTAG browser client connected');
        }).catch(error => {
            log('❌ JTAG connection failed: ' + error.message);
        });
    </script>
</body>
</html>`;
    
    res.setHeader('Content-Type', 'text/html');
    res.send(dashboardHTML);
  }

  /**
   * Serve browser client script
   */
  private serveBrowserClient(res: Response): void {
    res.setHeader('Content-Type', 'application/javascript');
    // In a real implementation, this would serve the compiled browser bundle
    res.send(`
      // JTAG Browser Client (placeholder)
      window.jtag = {
        connect: async () => {
          console.log('🔌 JTAG: Browser client connected');
          return { commands: { 
            screenshot: async (options) => ({ success: true, options }),
            health: async () => ({ status: 'healthy' }),
            list: async () => ({ commands: ['screenshot', 'health', 'list'] })
          }};
        }
      };
    `);
  }

  /**
   * Check if we should inject the browser client
   */
  private shouldInjectClient(req: Request, res: Response): boolean {
    return req.method === 'GET' && 
           req.accepts('html') &&
           !req.path.startsWith(this.options.pathPrefix);
  }

  /**
   * Inject browser client into HTML responses
   */
  private injectBrowserClient(req: Request, res: Response, next: NextFunction): void {
    const originalSend = res.send;
    
    res.send = function(body: any) {
      if (typeof body === 'string' && body.includes('</head>')) {
        // Inject JTAG browser client before </head>
        const injection = `
          <script>
            // JTAG Auto-Injection
            if (!window.jtag) {
              const script = document.createElement('script');
              script.src = '${this.options.pathPrefix}/browser-client.js';
              script.onload = () => console.log('🚀 JTAG: Auto-loaded for debugging');
              document.head.appendChild(script);
            }
          </script>
        `;
        body = body.replace('</head>', injection + '</head>');
      }
      return originalSend.call(this, body);
    }.bind(res);
    
    next();
  }
}

/**
 * Create JTAG middleware (Google Analytics style)
 */
export function middleware(options?: JTAGMiddlewareOptions) {
  return new JTAGMiddleware(options).middleware();
}

/**
 * Auto-start JTAG (Fabric style - just require and it works)
 */
export function autoStart(options?: JTAGMiddlewareOptions): void {
  if (typeof require !== 'undefined' && require.main === module) {
    console.log('🚀 JTAG: Auto-start mode detected');
    // In auto-start mode, we'd hook into the main application
    // This would need more sophisticated detection of Express/Connect apps
  }
}

// Support CommonJS
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { middleware, autoStart };
}

export default { middleware, autoStart };