/**
 * Renderer Daemon - Clean Modular Implementation
 * Uses focused components: HTMLRenderingEngine, TypeScriptCompiler, VersionService
 */

import { MessageRoutedDaemon, MessageRouteMap, MessageRouteHandler } from '../base/MessageRoutedDaemon.js';
import { DaemonResponse } from '../base/DaemonProtocol.js';
import { DaemonType } from '../base/DaemonTypes';
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

export class RendererDaemon extends MessageRoutedDaemon {
  public readonly name = 'renderer';
  public readonly version = '1.0.0';
  public readonly daemonType = DaemonType.RENDERER;

  private htmlEngine: HTMLRenderingEngine;
  private tsCompiler: TypeScriptCompiler;
  private versionService: VersionService;
  
  // MessageRoutedDaemon implementation
  protected readonly primaryMessageType = 'render_request';
  
  protected getRouteMap(): MessageRouteMap {
    return {
      'render_ui': this.handleRenderUI.bind(this),
      'update_component': this.handleUpdateComponent.bind(this),
      'render_page': this.handleRenderPage.bind(this)
    };
  }

  protected getAdditionalMessageHandlers(): { [messageType: string]: MessageRouteHandler } {
    return {
      'http_request': this.handleHttpRequest.bind(this),
      'get_capabilities': this.getCapabilities.bind(this)
    };
  }

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
          { pattern: '/src/*', handler: 'serve_file' },
          { pattern: '/dist/*', handler: 'serve_file' },
          { pattern: '/node_modules/*', handler: 'serve_file' }
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


  private async getCapabilities(): Promise<DaemonResponse> {
    return {
      success: true,
      data: {
        capabilities: [
          'ui_generation',
          'typescript_compilation',
          'template_rendering',
          'version_management'
        ],
        messageTypes: this.getSupportedMessageTypes(),
        routes: this.getSupportedRoutes()
      }
    };
  }

  // Route handlers
  private async handleRenderUI(request: RenderRequest): Promise<DaemonResponse> {
    return await this.handleRenderRequest(request);
  }

  private async handleUpdateComponent(request: RenderRequest): Promise<DaemonResponse> {
    return await this.handleRenderRequest(request);
  }

  private async handleRenderPage(request: RenderRequest): Promise<DaemonResponse> {
    return await this.handleRenderRequest(request);
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
      // Default to rendering UI for root path
      if (pathname === '/' || !handler) {
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
      }
      
      switch (handler) {
        case 'render_ui': {
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
        }

        case 'render_ui_components': {
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
        }

        case 'serve_ui_component':
         {
           // Serve widget files dynamically based on pathname
          const fs2 = await import('fs');
          const path2 = await import('path');
          const { fileURLToPath: fileURLToPath2 } = await import('url');
          
          const __dirname2 = path2.dirname(fileURLToPath2(import.meta.url));
          // Convert /src/ui/components/Chat/ChatWidget.js to actual file path
          const relativePath = pathname.replace(/^\/src\/ui\//, '');
          let componentPath = path2.join(__dirname2, '../../ui/', relativePath);
          
          // Try .ts extension if .js doesn't exist
          if (!fs2.existsSync(componentPath) && pathname.endsWith('.js')) {
            const tsPath = componentPath.replace(/\.js$/, '.ts');
            if (fs2.existsSync(tsPath)) {
              // Compile TypeScript to JavaScript on the fly
              const content = fs2.readFileSync(tsPath, 'utf-8');
              const compiledJS = await this.tsCompiler.compileWidgetComponent(content, tsPath);
              return {
                success: true,
                data: {
                  contentType: 'application/javascript',
                  content: compiledJS,
                  headers: { 'Cache-Control': 'no-cache, no-store, must-revalidate' }
                }
              };
            }
          }
          
          // Serve existing JS file
          if (fs2.existsSync(componentPath)) {
            const content = fs2.readFileSync(componentPath, 'utf-8');
            return {
              success: true,
              data: {
                contentType: 'application/javascript',
                content,
                headers: { 'Cache-Control': 'no-cache, no-store, must-revalidate' }
              }
            };
          }
          
          return {
            success: false,
            error: `Component not found: ${pathname}`
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