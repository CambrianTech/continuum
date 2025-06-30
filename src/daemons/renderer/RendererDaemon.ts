/**
 * Renderer Daemon - Encapsulates UI rendering system
 * Currently wraps legacy UIGenerator.cjs, can be swapped for modern TypeScript later
 * 
 * CRITICAL TESTING REQUIREMENTS:
 * ===============================
 * INTEGRATION TEST COVERAGE NEEDED:
 * - HTML output validation: Verify localhost:9000 serves expected UI structure
 * - Static file serving: Test /src/* and /dist/* route handling with proper MIME types
 * - Version injection: Verify cache-busting parameters added to script tags
 * - Error fallback: Test clean UI generation when clean-continuum-ui.html missing
 * - WebSocket registration: Verify routes properly registered with WebSocketDaemon
 * - Memory leak detection: Monitor for file handle/connection leaks during serving
 * 
 * LOGGING STRATEGY FOR FAILURE DETECTION:
 * - File serving errors with full path resolution traces
 * - UI generation timing metrics for performance regression detection
 * - Cache header validation logs for browser caching verification
 * - Route registration success/failure with WebSocketDaemon integration status
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
      this.log(`🔧 Current working directory: ${process.cwd()}`);
      
      // Verify file exists before attempting to read
      if (!fs.existsSync(sophisticatedUIPath)) {
        throw new Error(`CRITICAL: clean-continuum-ui.html not found at ${sophisticatedUIPath}`);
      }
      
      // Check file permissions
      try {
        fs.accessSync(sophisticatedUIPath, fs.constants.R_OK);
        this.log(`✅ File permissions OK: ${sophisticatedUIPath}`);
      } catch (permError) {
        throw new Error(`CRITICAL: Cannot read clean-continuum-ui.html - permission denied: ${permError.message}`);
      }
      
      const cleanHTML = fs.readFileSync(sophisticatedUIPath, 'utf8');
      this.log(`🔧 Loaded ${cleanHTML.length} characters of clean TypeScript UI`);
      
      // Create a renderer that serves the clean TypeScript UI
      this.legacyRenderer = {
        generateHTML: () => {
          // Return the clean HTML with version injection from package.json
          let html = cleanHTML;
          
          // Inject version from package.json
          html = html.replace('{{CONTINUUM_VERSION}}', this.dynamicVersion);
          
          // Update title if needed
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
      this.log(`❌ SEVERE RENDERING FAILURE: Failed to load clean TypeScript UI: ${errorMessage}`, 'error');
      this.log(`❌ SEVERE: Working directory: ${process.cwd()}`, 'error');
      this.log(`❌ SEVERE: Expected path: ${path.join(process.cwd(), 'clean-continuum-ui.html')}`, 'error');
      this.log(`❌ SEVERE: This should NEVER happen in normal operation!`, 'error');
      this.log('🚨 EMERGENCY FALLBACK: Using static HTML (this indicates severe system failure)...');
      
      // Log file system state for debugging
      try {
        const fs = await import('fs');
        const files = fs.readdirSync(process.cwd());
        this.log(`📁 Files in working directory: ${files.filter(f => f.includes('html')).join(', ')}`, 'error');
      } catch (fsError) {
        this.log(`❌ Cannot even read directory: ${fsError.message}`, 'error');
      }
      
      // CRITICAL: Primary UI loading failed - provide simple error response
      this.log('🚨 SYSTEM FAILURE: RendererDaemon cannot function without clean-continuum-ui.html', 'error');
      this.log('🚨 Providing simple error page instead of complex fallback', 'error');
      
      // Load error page template and inject error details
      const errorHTML = await this.loadErrorPageTemplate(errorMessage, error instanceof Error ? error.stack : 'No stack trace available');

      this.legacyRenderer = {
        generateHTML: () => errorHTML,
        cleanup: () => Promise.resolve(),
        setCommandRouter: () => {}
      };
      
      this.log('✅ Simple error page loaded (system configuration failure)');
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

  private async renderWithModern(_request: RenderRequest): Promise<RenderResult> {
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
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to switch engine: ${errorMessage}`
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
  private async handleUIRoute(_pathname: string, _req: any, res: any): Promise<void> {
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
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Failed to serve UI: ${errorMessage}`, 'error');
      res.writeHead(500, { 'Content-Type': 'text/plain' });
      res.end('Error loading UI from RendererDaemon');
    }
  }

  // NOTE: API endpoints moved to DataAPIDaemon
  // agents/personas data should not be hardcoded in rendering daemon

  /**
   * Handle static file routes - this method is called by WebSocketDaemon
   */
  private async handleStaticRoute(pathname: string, _req: any, res: any): Promise<void> {
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
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Failed to serve static file ${pathname}: ${errorMessage}`, 'error');
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
      'ts': 'application/javascript', // TypeScript transpiled to JS
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

  // TODO: Static file server management - delegated to WebSocketDaemon  
  // This method will be removed once full transition to WebSocketDaemon routing is complete
  private async __unused_startStaticFileServer(): Promise<void> {
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
      const ext = pathname.split('.').pop() || '';
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
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`Failed to serve static file ${pathname}: ${errorMessage}`, 'error');
      res.writeHead(404, { 'Content-Type': 'text/plain' });
      res.end('Not Found');
    }
  }


  /**
   * Load error page template and inject error details
   */
  private async loadErrorPageTemplate(errorMessage: string, stackTrace: string): Promise<string> {
    try {
      const fs = await import('fs');
      const path = await import('path');
      
      const templatePath = path.join(__dirname, 'templates', 'error-page.html');
      const template = fs.readFileSync(templatePath, 'utf8');
      
      // Inject error details into template
      return template
        .replace('{{ERROR_MESSAGE}}', errorMessage)
        .replace('{{TIMESTAMP}}', new Date().toISOString())
        .replace('{{WORKING_DIR}}', process.cwd())
        .replace('{{STACK_TRACE}}', stackTrace);
        
    } catch (templateError) {
      // Fallback to minimal HTML if template loading fails
      this.log(`❌ Failed to load error template: ${templateError.message}`, 'error');
      return `<!DOCTYPE html>
<html><head><title>Continuum - Critical System Failure</title></head>
<body style="font-family: monospace; background: #111; color: #ff6666; padding: 20px;">
<h1>🚨 CRITICAL SYSTEM FAILURE</h1>
<p>Primary UI failed: ${errorMessage}</p>
<p>Error template also failed: ${templateError.message}</p>
<p>Time: ${new Date().toISOString()}</p>
<pre>${stackTrace}</pre>
</body></html>`;
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

// Main execution when run directly (ES module compatible)
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