/**
 * Renderer Daemon - Clean Modular Implementation
 * Uses focused components: HTMLRenderingEngine, TypeScriptCompiler, VersionService
 */

import { BaseDaemon } from '../base/BaseDaemon';
import { DaemonMessage, DaemonResponse } from '../base/DaemonProtocol';
import { HTMLRenderingEngine } from './core/HTMLRenderingEngine';
import { TypeScriptCompiler } from './core/TypeScriptCompiler';
import { VersionService } from './core/VersionService';

export interface RenderRequest {
  readonly type: 'render_ui' | 'update_component' | 'render_page';
  readonly data: Record<string, any>;
  readonly clientId?: string;
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

  private htmlEngine: HTMLRenderingEngine;
  private tsCompiler: TypeScriptCompiler;
  private versionService: VersionService;
  private webSocketDaemon: any = null;

  constructor() {
    super();
    this.htmlEngine = new HTMLRenderingEngine();
    this.tsCompiler = new TypeScriptCompiler();
    this.versionService = new VersionService();
  }

  protected async onStart(): Promise<void> {
    this.log('🎨 Starting clean modular Renderer Daemon...');
    this.log('✅ Renderer Daemon started with modular architecture');
  }

  protected async onStop(): Promise<void> {
    this.log('🛑 Stopping Renderer Daemon...');
  }

  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    try {
      switch (message.type) {
        case 'render_request':
          return await this.handleRenderRequest(message.data as RenderRequest);
        
        case 'get_capabilities':
          return {
            success: true,
            data: {
              ui_generation: true,
              typescript_compilation: true,
              template_rendering: true,
              version_management: true
            }
          };

        default:
          return {
            success: false,
            error: `Unknown message type: ${message.type}`
          };
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to handle message: ${errorMessage}`
      };
    }
  }

  private async handleRenderRequest(request: RenderRequest): Promise<DaemonResponse> {
    try {
      const version = await this.versionService.getCurrentVersion();
      
      const html = await this.htmlEngine.renderMainUI({
        version,
        templatePath: request.data.templatePath
      });

      return {
        success: true,
        data: {
          html,
          version
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      const stackTrace = error instanceof Error ? (error.stack || 'No stack trace') : 'No stack trace';
      
      const errorHTML = await this.htmlEngine.renderErrorPage(errorMessage, stackTrace);
      
      return {
        success: true,
        data: {
          html: errorHTML,
          error: errorMessage
        }
      };
    }
  }

  /**
   * Register routes with WebSocketDaemon - clean interface
   */
  public registerWithWebSocketDaemon(webSocketDaemon: any): void {
    this.webSocketDaemon = webSocketDaemon;
    
    // Register clean route handlers
    this.webSocketDaemon.registerRouteHandler('/', this, this.handleUIRoute.bind(this));
    this.webSocketDaemon.registerRouteHandler('/dist/api.js', this, this.handleAPIRoute.bind(this));
    this.webSocketDaemon.registerRouteHandler('/node_modules/continuum/dist/api.js', this, this.handleAPIRoute.bind(this));
    
    this.log('🔌 Registered clean routes with WebSocketDaemon');
  }

  private async handleUIRoute(_pathname: string, _req: any, res: any): Promise<void> {
    try {
      const version = await this.versionService.getCurrentVersion();
      const html = await this.htmlEngine.renderMainUI({ version });
      
      res.writeHead(200, { 
        'Content-Type': 'text/html',
        'Cache-Control': 'no-cache, no-store, must-revalidate'
      });
      res.end(html);
      
      this.log(`✅ Served main UI (v${version})`);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Failed to serve UI: ${errorMessage}`, 'error');
      
      res.writeHead(500, { 'Content-Type': 'text/plain' });
      res.end('Error loading UI');
    }
  }

  private async handleAPIRoute(_pathname: string, _req: any, res: any): Promise<void> {
    try {
      const version = await this.versionService.getCurrentVersion();
      const compiledJS = await this.tsCompiler.compileContinuumAPI({ version });
      
      res.writeHead(200, {
        'Content-Type': 'application/javascript',
        'Cache-Control': 'no-cache, no-store, must-revalidate'
      });
      res.end(compiledJS);
      
      this.log(`✅ Served compiled API (${compiledJS.length} chars)`);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ API compilation failed: ${errorMessage}`, 'error');
      
      res.writeHead(500, { 'Content-Type': 'text/plain' });
      res.end(`API compilation error: ${errorMessage}`);
    }
  }
}

// Main execution
if (require.main === module) {
  const daemon = new RendererDaemon();
  
  process.on('SIGINT', async () => {
    console.log('\n🛑 Received shutdown signal...');
    await daemon.stop();
    process.exit(0);
  });

  daemon.start().catch(error => {
    console.error('❌ Failed to start Renderer Daemon:', error);
    process.exit(1);
  });
}