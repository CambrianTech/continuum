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

  public async registerRoutesWithWebSocket(): Promise<void> {
    return new Promise((resolve, reject) => {
      const WebSocket = require('ws');
      const ws = new WebSocket('ws://localhost:9000');
      
      ws.on('open', () => {
        // Register routes through WebSocket messages
        const routeRegistrations = [
          { pattern: '/', handler: 'render_ui' },
          { pattern: '/src/ui/continuum.js', handler: 'render_api' },
          { pattern: '/dist/api.js', handler: 'render_api' },
          { pattern: '/node_modules/continuum/dist/api.js', handler: 'render_api' }
        ];

        const registrationMessage = {
          type: 'register_http_routes',
          daemon: 'renderer',
          routes: routeRegistrations
        };

        ws.send(JSON.stringify(registrationMessage));
        this.log('📨 Sent route registration to WebSocket daemon');
        ws.close();
        resolve();
      });

      ws.on('error', (error: any) => {
        this.log(`❌ Failed to register routes with WebSocket: ${error}`, 'error');
        reject(error);
      });
    });
  }

  protected async onStop(): Promise<void> {
    this.log('🛑 Stopping Renderer Daemon...');
  }

  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    try {
      switch (message.type) {
        case 'render_request':
          return await this.handleRenderRequest(message.data as RenderRequest);
        
        case 'http_request':
          return await this.handleHttpRequest(message.data);
        
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

  private async handleHttpRequest(data: any): Promise<DaemonResponse> {
    const { pathname, handler } = data;
    
    try {
      switch (handler) {
        case 'render_ui':
          const version = await this.versionService.getCurrentVersion();
          const html = await this.htmlEngine.renderMainUI({ version });
          return {
            success: true,
            data: {
              contentType: 'text/html',
              content: html,
              headers: { 'Cache-Control': 'no-cache, no-store, must-revalidate' }
            }
          };

        case 'render_api':
          const apiVersion = await this.versionService.getCurrentVersion();
          const compiledJS = await this.tsCompiler.compileContinuumAPI({ version: apiVersion });
          return {
            success: true,
            data: {
              contentType: 'application/javascript',
              content: compiledJS,
              headers: { 'Cache-Control': 'no-cache, no-store, must-revalidate' }
            }
          };

        case 'render_ui_components':
          // Serve the existing continuum-browser.js file
          const fs = await import('fs');
          const path = await import('path');
          const { fileURLToPath } = await import('url');
          
          const __dirname = path.dirname(fileURLToPath(import.meta.url));
          const browserJSPath = path.join(__dirname, '../../ui/continuum-browser.js');
          
          if (fs.existsSync(browserJSPath)) {
            const content = fs.readFileSync(browserJSPath, 'utf-8');
            return {
              success: true,
              data: {
                contentType: 'application/javascript',
                content,
                headers: { 'Cache-Control': 'no-cache, no-store, must-revalidate' }
              }
            };
          } else {
            return {
              success: false,
              error: 'continuum-browser.js not found - run build first'
            };
          }


        default:
          return {
            success: false,
            error: `Unknown handler: ${handler}`
          };
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ HTTP request error for ${pathname}: ${errorMessage}`, 'error');
      
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * DEPRECATED: Use WebSocket message-based registration instead
   */
  public registerWithWebSocketDaemon(_webSocketDaemon: any): void {
    this.log('⚠️  Direct registration deprecated - using WebSocket messages instead');
  }
}

// Main execution (direct execution detection)
if (process.argv[1] && process.argv[1].endsWith('RendererDaemon.ts')) {
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