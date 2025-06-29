/**
 * Renderer Daemon - Encapsulates UI rendering system
 * Currently wraps legacy UIGenerator.cjs, can be swapped for modern TypeScript later
 * 
 * ARCHITECTURAL INSIGHTS FROM ERROR FIXING:
 * ==========================================
 * LEGACY INTEGRATION ISSUES:
 * - Heavy coupling to legacy UIGenerator.cjs (5000 lines!)
 * - Should extract rendering interfaces for better abstraction
 * - Error handling reveals tight coupling to file system operations
 * 
 * DISCOVERED REFACTORING OPPORTUNITIES:
 * - RenderRequest.data should be properly typed (not 'any')
 * - Version loading logic should be in separate VersionService
 * - Static file serving should delegate to existing WebSocketDaemon
 * - Legacy renderer loading suggests need for Renderer strategy pattern
 * 
 * TODO: Break into smaller, focused classes:
 * - HTMLRenderingEngine (pure rendering logic)
 * - StaticFileService (file serving abstraction)
 * - ComponentRegistry (widget management)
 * - VersionService (version detection/loading)
 */

import { BaseDaemon } from '../base/BaseDaemon';
import { DaemonMessage, DaemonResponse } from '../base/DaemonProtocol';
import * as http from 'http';
import * as path from 'path'; // TODO: Consider using URL-based paths for better cross-platform support

// TODO: Replace 'any' with proper typed interfaces
export interface RenderRequest {
  readonly type: 'render_ui' | 'update_component' | 'render_page';
  readonly data: RenderData; // TODO: Define specific interfaces for each render type
  readonly clientId?: string;
}

export interface RenderData {
  // TODO: Define proper union types for different render operations
  [key: string]: unknown;
}

export interface RenderResult {
  readonly success: boolean;
  readonly html?: string;
  readonly css?: string;
  readonly js?: string;
  readonly error?: string;
}

export class RendererDaemon extends BaseDaemon {
  public readonly name = 'renderer';
  public readonly version = '1.0.0';
  private dynamicVersion: string = '1.0.0';

  private legacyRenderer: any = null;
  private renderingEngine: 'legacy' | 'modern' = 'legacy';
  private httpServer: http.Server | null = null;
  private staticPort: number = 9001;
  private webSocketDaemon: any = null;

  protected async onStart(): Promise<void> {
    this.log('🎨 Starting Renderer Daemon...');
    
    // Load current version from package.json
    await this.loadCurrentVersion();
    
    try {
      if (this.renderingEngine === 'legacy') {
        this.log('📦 Attempting to load legacy renderer...');
        await this.loadLegacyRenderer();
        this.log('✅ Legacy renderer loaded successfully');
      } else {
        this.log('🚀 Attempting to load modern renderer...');
        await this.loadModernRenderer();
        this.log('✅ Modern renderer loaded successfully');
      }
      
      // Start static file server for widget assets
      // NOTE: No separate HTTP server - all routing through WebSocketDaemon
      this.log(`🗂️ Static file serving handled via WebSocketDaemon on port 9000`);
      
      this.log(`✅ Renderer Daemon started with ${this.renderingEngine} engine`);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      const errorStack = error instanceof Error ? error.stack : 'No stack trace available';
      this.log(`❌ Failed to start Renderer Daemon: ${errorMessage}`, 'error');
      this.log(`❌ Stack trace: ${errorStack}`, 'error');
      throw error;
    }
  }

  protected async onStop(): Promise<void> {
    this.log('🛑 Stopping Renderer Daemon...');
    
    if (this.legacyRenderer && this.legacyRenderer.cleanup) {
      await this.legacyRenderer.cleanup();
    }
    
    this.log('✅ Renderer Daemon stopped');
  }

  private async loadCurrentVersion(): Promise<void> {
    try {
      const { readFileSync } = await import('fs');
      const packageData = JSON.parse(readFileSync('./package.json', 'utf8'));
      this.dynamicVersion = packageData.version;
      this.log(`📦 Loaded current version: ${this.dynamicVersion}`);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`⚠️ Failed to load version from package.json: ${errorMessage}`, 'warn');
      this.dynamicVersion = '0.2.UNKNOWN';
    }
  }

  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    switch (message.type) {
      case 'render_request':
        return await this.handleRenderRequest(message.data);
        
      case 'switch_engine':
        return await this.switchRenderingEngine(message.data.engine);
        
      case 'get_capabilities':
        return {
          success: true,
          data: {
            engine: this.renderingEngine,
            capabilities: this.getCapabilities()
          }
        };
        
      default:
        return {
          success: false,
          error: `Unknown message type: ${message.type}`
        };
    }
  }

  private async handleRenderRequest(request: RenderRequest): Promise<DaemonResponse> {
    try {
      this.log(`🎨 Rendering: ${request.type}`);
      
      let result: RenderResult;
      
      if (this.renderingEngine === 'legacy') {
        result = await this.renderWithLegacy(request);
      } else {
        result = await this.renderWithModern(request);
      }
      
      return {
        success: result.success,
        data: result
      };
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Render error: ${errorMessage}`, 'error');
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  private async loadLegacyRenderer(): Promise<void> {
    try {
      this.log('📦 Loading clean TypeScript UI client...');
      
      // Load the clean TypeScript UI instead of the messy legacy HTML
      const fs = await import('fs');
      const path = await import('path');
      
      const sophisticatedUIPath = path.join(process.cwd(), 'clean-continuum-ui.html');
      this.log(`🔧 Reading clean TypeScript UI from: ${sophisticatedUIPath}`);
      
      const cleanHTML = fs.readFileSync(sophisticatedUIPath, 'utf8');
      this.log(`🔧 Loaded ${cleanHTML.length} characters of clean TypeScript UI`);
      
      // Create a renderer that serves the clean TypeScript UI
      this.legacyRenderer = {
        generateHTML: () => {
          // Return the clean HTML with minimal modifications
          let html = cleanHTML;
          
          // Just update version in title if needed
          html = html.replace(/Continuum - Clean TypeScript Client/, `Continuum v${this.dynamicVersion} - TypeScript Client`);
          
          return html;
        },
        
        // Mock other methods that might be called
        cleanup: () => Promise.resolve(),
        setCommandRouter: () => {
          this.log('🔧 Command router override set (disabled in daemon mode)');
        }
      };
      
      this.log('✅ Clean TypeScript UI renderer loaded successfully');
      this.log('🎨 Features: Modern TypeScript architecture, clean module imports, proper API communication');
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ CRITICAL: Failed to load clean TypeScript UI: ${errorMessage}`, 'error');
      this.log(`❌ CRITICAL: Working directory: ${process.cwd()}`, 'error');
      this.log(`❌ CRITICAL: Attempted path: ${path.join(process.cwd(), 'uigenerator.html')}`, 'error');
      this.log('🔄 Falling back to old TypeScript UI Generator (5000+ lines of mess)...');
      
      // Generate clean TypeScript UI directly (no CommonJS fallback)
      this.log('🚀 Generating clean TypeScript UI directly...');
      
      const cleanHTML = `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Continuum - TypeScript Architecture</title>
    <link rel="icon" href="data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 100 100'><text y='.9em' font-size='90'>🟢</text></svg>">
    
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { 
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', sans-serif;
            background: linear-gradient(135deg, #0f1419 0%, #1a1f2e 100%);
            color: #e0e6ed; height: 100vh; overflow: hidden;
        }
        .app-container { display: flex; height: 100vh; }
        .sidebar { width: 350px; background: rgba(20, 25, 35, 0.95); border-right: 1px solid rgba(255, 255, 255, 0.1); padding: 20px; }
        .main-content { flex: 1; display: flex; flex-direction: column; }
        .chat-container { flex: 1; padding: 20px; overflow-y: auto; }
        .input-area { padding: 20px; border-top: 1px solid rgba(255, 255, 255, 0.1); background: rgba(20, 25, 35, 0.9); }
        .input-container { display: flex; gap: 12px; align-items: flex-end; }
        .input-field { flex: 1; background: rgba(255, 255, 255, 0.08); border: 1px solid rgba(255, 255, 255, 0.2); border-radius: 24px; padding: 14px 20px; color: #e0e6ed; font-size: 16px; resize: none; outline: none; }
        .send-button { width: 48px; height: 48px; background: linear-gradient(135deg, #4FC3F7, #29B6F6); border: none; border-radius: 50%; color: white; cursor: pointer; display: flex; align-items: center; justify-content: center; font-size: 18px; }
        .status-indicator { width: 12px; height: 12px; border-radius: 50%; margin-right: 8px; }
    </style>
</head>
<body>
    <div class="app-container">
        <div class="sidebar">
            <h2>Continuum</h2>
            <div style="margin-top: 20px;">
                <div style="display: flex; align-items: center; margin-bottom: 10px;">
                    <div id="status-indicator" class="status-indicator" style="background: #FF9800;"></div>
                    <span id="status-text">Connecting...</span>
                </div>
            </div>
        </div>
        
        <div class="main-content">
            <div class="chat-container" id="chat-container">
                <div style="margin-bottom: 15px; padding: 12px; border-radius: 8px; background: rgba(255, 255, 255, 0.08); border: 1px solid rgba(255, 255, 255, 0.1); max-width: 70%;">
                    <div>Welcome to Continuum! Pure TypeScript architecture loaded.</div>
                </div>
            </div>
            
            <div class="input-area">
                <div class="input-container">
                    <textarea id="messageInput" class="input-field" placeholder="Type your message..." rows="1"></textarea>
                    <button class="send-button" id="sendButton">➤</button>
                </div>
            </div>
        </div>
    </div>

    <!-- PURE TYPESCRIPT - NO COMMONJS HELL -->
    <script type="module">
        import { uiManager } from '/src/ui/UIManager.js';
        import { browserCommandProcessor } from '/src/ui/BrowserCommandProcessor.js';
        
        console.log('🚀 Pure TypeScript client loading...');
        
        async function initializeClient() {
            try {
                await uiManager.connect('ws://localhost:9000');
                document.getElementById('status-text').textContent = 'Connected';
                document.getElementById('status-indicator').style.background = '#4CAF50';
                console.log('✅ Pure TypeScript client connected');
            } catch (error) {
                document.getElementById('status-text').textContent = 'Failed';
                document.getElementById('status-indicator').style.background = '#F44336';
                console.error('❌ TypeScript client failed:', error);
            }
        }
        
        initializeClient();
    </script>
</body>
</html>`;
      
      const UIGeneratorClass = class {
        generateHTML() { return cleanHTML; }
      };
      const mockContinuum = {
        costTracker: {
          getTotalCost: () => 0,
          getSessionCost: () => 0,
          getRequestCount: () => 0,
          getRequests: () => 0,
          getTotal: () => 0
        },
        port: 9000,
        version: this.version
      };
      // TODO: Legacy UIGenerator coupling - should use dependency injection
      this.legacyRenderer = new UIGeneratorClass(); // Constructor expects 0 args
      this.log('✅ Fallback renderer loaded');
    }
  }

  private async loadModernRenderer(): Promise<void> {
    this.log('🚀 Loading modern TypeScript renderer...');
    
    // TODO: Implement modern TypeScript renderer
    // For now, fallback to legacy
    this.renderingEngine = 'legacy';
    await this.loadLegacyRenderer();
    
    this.log('⚠️ Modern renderer not implemented, using legacy fallback');
  }

  private async renderWithLegacy(request: RenderRequest): Promise<RenderResult> {
    if (!this.legacyRenderer) {
      return {
        success: false,
        error: 'Legacy renderer not loaded'
      };
    }

    try {
      this.log('🎨 Calling legacy renderer generateHTML method...');
      
      // Use the legacy renderer for UI generation
      // UIGenerator.cjs only has generateHTML() method, not generateCSS/generateJS
      const html = this.legacyRenderer.generateHTML(request.data);
      this.log('✅ Legacy renderer generated HTML successfully');
      
      return {
        success: true,
        html,
        // TODO: Legacy renderer embeds CSS/JS - extract to separate properties for cleaner architecture
        css: '', // Legacy renderer embeds CSS in HTML
        js: ''   // Legacy renderer embeds JS in HTML
      };
      
    } catch (error) {
      // TODO: Same error pattern as 5+ other places - extract BaseErrorHandler
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Legacy renderer failed: ${errorMessage}`, 'error');
      return {
        success: false,
        error: `Legacy renderer error: ${errorMessage}`
      };
    }
  }

  private async renderWithModern(request: RenderRequest): Promise<RenderResult> {
    // TODO: Implement modern TypeScript rendering
    return {
      success: false,
      error: 'Modern renderer not implemented yet'
    };
  }

  private async switchRenderingEngine(engine: 'legacy' | 'modern'): Promise<DaemonResponse> {
    try {
      this.log(`🔄 Switching rendering engine to: ${engine}`);
      
      // Stop current engine
      if (this.legacyRenderer && this.legacyRenderer.cleanup) {
        await this.legacyRenderer.cleanup();
      }
      
      // Switch engine
      this.renderingEngine = engine;
      
      // Load new engine
      if (engine === 'legacy') {
        await this.loadLegacyRenderer();
      } else {
        await this.loadModernRenderer();
      }
      
      this.log(`✅ Switched to ${this.renderingEngine} rendering engine`);
      
      return {
        success: true,
        data: { engine: this.renderingEngine }
      };
      
    } catch (error) {
      return {
        success: false,
        error: `Failed to switch engine: ${error.message}`
      };
    }
  }

  /**
   * Register static file routes with WebSocketDaemon
   * Called when RendererDaemon is registered as an external daemon
   */
  public registerWithWebSocketDaemon(webSocketDaemon: any): void {
    this.webSocketDaemon = webSocketDaemon;
    
    // Register route handlers for static files
    this.webSocketDaemon.registerRouteHandler('/src/*', this, this.handleStaticRoute.bind(this));
    this.webSocketDaemon.registerRouteHandler('/dist/*', this, this.handleStaticRoute.bind(this));
    
    // Register the root UI serving route
    this.webSocketDaemon.registerRouteHandler('/', this, this.handleUIRoute.bind(this));
    
    this.log('🔌 Registered routes and APIs with WebSocketDaemon');
  }

  /**
   * Handle UI serving route - serve the main application
   */
  private async handleUIRoute(pathname: string, req: any, res: any): Promise<void> {
    try {
      // Use our legacy renderer to generate the UI
      let html = this.legacyRenderer.generateHTML({});
      
      // Inject current version into script tags for cache busting
      const timestamp = Date.now();
      html = html.replace(
        /src="([^"?]+\.js)(\?[^"]*)?"/g, 
        `src="$1?v=${this.dynamicVersion}&bust=${timestamp}"`
      );
      
      // Add aggressive no-cache headers for the HTML itself
      res.writeHead(200, { 
        'Content-Type': 'text/html',
        'Cache-Control': 'no-cache, no-store, must-revalidate, proxy-revalidate',
        'Pragma': 'no-cache',
        'Expires': '0',
        'Surrogate-Control': 'no-store',
        'ETag': `"${this.dynamicVersion}-${timestamp}"`,
        'Last-Modified': new Date().toUTCString()
      });
      res.end(html);
      this.log(`✅ Served main UI via RendererDaemon (v${this.dynamicVersion})`);
    } catch (error) {
      this.log(`❌ Failed to serve UI: ${error.message}`, 'error');
      res.writeHead(500, { 'Content-Type': 'text/plain' });
      res.end('Error loading UI from RendererDaemon');
    }
  }

  // NOTE: API endpoints moved to DataAPIDaemon
  // agents/personas data should not be hardcoded in rendering daemon

  /**
   * Handle static file routes - this method is called by WebSocketDaemon
   */
  private async handleStaticRoute(pathname: string, req: any, res: any): Promise<void> {
    try {
      // Serve static files directly instead of proxying
      const { promises: fs } = await import('fs');
      const { join, extname } = await import('path');

      // Remove query parameters and leading slash
      const cleanPath = pathname.split('?')[0].substring(1);
      const fullPath = join(process.cwd(), cleanPath);

      // Security check - ensure path is within project
      if (!fullPath.startsWith(process.cwd())) {
        res.writeHead(403, { 'Content-Type': 'text/plain' });
        res.end('Forbidden');
        return;
      }

      // Check if file exists
      await fs.access(fullPath);
      const stats = await fs.stat(fullPath);

      if (!stats.isFile()) {
        this.log(`❌ Path is not a file: ${pathname}`, 'error');
        res.writeHead(404, { 'Content-Type': 'text/plain' });
        res.end('Not Found');
        return;
      }

      // Determine content type
      const contentType = this.getContentType(extname(fullPath));

      // Read and serve file
      const content = await fs.readFile(fullPath);
      
      res.writeHead(200, {
        'Content-Type': contentType,
        'Content-Length': content.length,
        'Cache-Control': 'no-cache, no-store, must-revalidate',
        'Pragma': 'no-cache',
        'Expires': '0'
      });
      res.end(content);

      this.log(`✅ Served static file: ${pathname} (${content.length} bytes, ${contentType})`);

    } catch (error) {
      this.log(`❌ Failed to serve static file ${pathname}: ${error.message}`, 'error');
      res.writeHead(404, { 'Content-Type': 'text/plain' });
      res.end('Not Found');
    }
  }

  private getContentType(ext: string): string {
    // Handle both .js and js formats
    const cleanExt = ext.startsWith('.') ? ext.substring(1) : ext;
    
    const mimeTypes: Record<string, string> = {
      'js': 'application/javascript',
      'mjs': 'application/javascript',
      'css': 'text/css',
      'html': 'text/html',
      'json': 'application/json',
      'ts': 'application/javascript', // TypeScript served as JS
      'map': 'application/json'
    };

    return mimeTypes[cleanExt.toLowerCase()] || 'application/javascript';
  }

  private getCapabilities(): string[] {
    const capabilities = ['basic-rendering', 'static-file-serving'];
    
    if (this.renderingEngine === 'legacy') {
      capabilities.push('legacy-ui', 'cyberpunk-theme');
    } else {
      capabilities.push('modern-ui', 'typescript-components');
    }
    
    return capabilities;
  }

  private async startStaticFileServer(): Promise<void> {
    this.log(`🗂️ Starting static file server on port ${this.staticPort}...`);
    
    this.httpServer = http.createServer(async (req, res) => {
      await this.handleStaticFileRequest(req, res);
    });

    return new Promise((resolve, reject) => {
      this.httpServer!.listen(this.staticPort, () => {
        this.log(`✅ Static file server listening on http://localhost:${this.staticPort}`);
        resolve();
      });

      this.httpServer!.on('error', (error) => {
        this.log(`❌ Static file server error: ${error.message}`, 'error');
        reject(error);
      });
    });
  }

  private async handleStaticFileRequest(req: http.IncomingMessage, res: http.ServerResponse): Promise<void> {
    const url = new URL(req.url!, `http://localhost:${this.staticPort}`);
    
    // Only serve files from /src/ and /dist/ paths
    if (url.pathname.startsWith('/src/') || url.pathname.startsWith('/dist/')) {
      await this.serveStaticFile(url.pathname, res, req);
    } else {
      res.writeHead(404, { 'Content-Type': 'text/plain' });
      res.end('Not Found - RendererDaemon only serves /src/ and /dist/ paths');
    }
  }

  private async serveStaticFile(pathname: string, res: http.ServerResponse, req?: http.IncomingMessage): Promise<void> {
    const { readFile, stat } = await import('fs/promises');
    const { join } = await import('path');
    const crypto = await import('crypto');
    
    try {
      // Remove leading slash and construct file path
      const filePath = join(process.cwd(), pathname.substring(1));
      
      // Get file stats for Last-Modified and ETag
      const stats = await stat(filePath);
      const lastModified = stats.mtime.toUTCString();
      const etag = `"${crypto.createHash('md5')
        .update(`${stats.size}-${stats.mtime.getTime()}`)
        .digest('hex')}"`;
      
      // Check if client has cached version
      if (req) {
        const ifModifiedSince = req.headers['if-modified-since'];
        const ifNoneMatch = req.headers['if-none-match'];
        
        if ((ifModifiedSince && ifModifiedSince === lastModified) ||
            (ifNoneMatch && ifNoneMatch === etag)) {
          res.writeHead(304); // Not Modified
          res.end();
          this.log(`📋 Cache hit for ${pathname} (304 Not Modified)`);
          return;
        }
      }
      
      // Determine content type
      const ext = pathname.split('.').pop();
      const contentType = this.getContentType(ext);
      
      // Set caching headers based on file type
      const cacheHeaders = this.getCacheHeaders(ext);
      
      const content = await readFile(filePath);
      res.writeHead(200, {
        'Content-Type': contentType,
        'Last-Modified': lastModified,
        'ETag': etag,
        'Access-Control-Allow-Origin': '*', // Allow CORS for widget loading
        ...cacheHeaders
      });
      res.end(content);
      
      this.log(`📋 Served ${pathname} with caching headers (${content.length} bytes)`);
    } catch (error) {
      this.log(`Failed to serve static file ${pathname}: ${error.message}`, 'error');
      res.writeHead(404, { 'Content-Type': 'text/plain' });
      res.end('Not Found');
    }
  }


  private getCacheHeaders(ext: string | undefined): Record<string, string> {
    const cacheSettings: Record<string, Record<string, string>> = {
      // Long cache for static assets (CSS/JS)
      'css': {
        'Cache-Control': 'public, max-age=31536000, immutable', // 1 year
        'Expires': new Date(Date.now() + 31536000000).toUTCString()
      },
      'js': {
        'Cache-Control': 'no-cache, no-store, must-revalidate',
        'Pragma': 'no-cache',
        'Expires': '0'
      },
      'ts': {
        'Cache-Control': 'public, max-age=31536000, immutable', // 1 year
        'Expires': new Date(Date.now() + 31536000000).toUTCString()
      },
      // Medium cache for images
      'png': { 'Cache-Control': 'public, max-age=2592000' }, // 30 days
      'jpg': { 'Cache-Control': 'public, max-age=2592000' },
      'jpeg': { 'Cache-Control': 'public, max-age=2592000' },
      'gif': { 'Cache-Control': 'public, max-age=2592000' },
      'svg': { 'Cache-Control': 'public, max-age=2592000' },
      // Short cache for dynamic content
      'html': { 'Cache-Control': 'public, max-age=300' }, // 5 minutes
      'json': { 'Cache-Control': 'public, max-age=300' }
    };
    
    return cacheSettings[ext || ''] || { 'Cache-Control': 'public, max-age=3600' }; // 1 hour default
  }
}

// Main execution when run directly
if (import.meta.url === `file://${process.argv[1]}`) {
  const daemon = new RendererDaemon();
  
  process.on('SIGINT', async () => {
    console.log('\n🛑 Received shutdown signal...');
    await daemon.stop();
    process.exit(0);
  });
  
  process.on('SIGTERM', async () => {
    console.log('\n🛑 Received termination signal...');
    await daemon.stop();
    process.exit(0);
  });
  
  daemon.start().catch(error => {
    console.error('❌ Renderer daemon failed:', error);
    process.exit(1);
  });
}