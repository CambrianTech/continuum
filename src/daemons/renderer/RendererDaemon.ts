/**
 * Renderer Daemon - Clean Modular Implementation
 * Uses focused components: HTMLRenderingEngine, TypeScriptCompiler, VersionService
 */

import { MessageRoutedDaemon, MessageRouteMap, MessageRouteHandler } from '../base/MessageRoutedDaemon';
import { DaemonResponse } from '../base/DaemonProtocol';
import { DaemonType } from '../base/DaemonTypes';
import { HTMLRenderingEngine } from './core/HTMLRenderingEngine';
import { VersionService } from './core/VersionService';
import { widgetManager } from '../../ui/components/core/WidgetManager';

export interface RenderRequest {
  readonly type: 'render_ui' | 'update_component' | 'render_page';
  readonly data: Record<string, unknown>;
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
  private versionService: VersionService;
  
  // MessageRoutedDaemon implementation
  protected readonly primaryMessageType = 'render_request';
  
  protected getRouteMap(): MessageRouteMap {
    return {
      'render_ui': (data: unknown) => this.handleRenderUI(data as RenderRequest),
      'update_component': (data: unknown) => this.handleUpdateComponent(data as RenderRequest),
      'render_page': (data: unknown) => this.handleRenderPage(data as RenderRequest),
      'render_widget': (data: unknown) => this.handleRenderWidget(data as { name: string }),
      'get_widget_assets': (data: unknown) => this.handleGetWidgetAssets(data as { name: string }),
      'get_widget_script': (data: unknown) => this.handleGetWidgetScript(data as { name: string }),
      'get_widget_styles': (data: unknown) => this.handleGetWidgetStyles(data as { name: string })
    };
  }

  protected getAdditionalMessageHandlers(): { [messageType: string]: MessageRouteHandler<unknown> } {
    return {
      'http_request': (data: unknown) => this.handleHttpRequest(data as { pathname: string; handler: string }),
      'get_capabilities': (_data: unknown) => this.getCapabilities()
    };
  }

  constructor() {
    super();
    this.htmlEngine = new HTMLRenderingEngine();
    this.versionService = new VersionService();
  }

  protected async onStart(): Promise<void> {
    this.log('🎨 Starting clean modular Renderer Daemon...');
    
    // Initialize widget system
    try {
      this.log('🔧 Initializing widget system...');
      await widgetManager.initialize({
        baseUrl: 'http://localhost:9000',
        assetsPath: '/src/ui/components',
        compilationEnabled: true,
        developmentMode: true
      });
      this.log('✅ Widget system initialized successfully');
    } catch (error) {
      this.log(`❌ Widget system initialization failed: ${error}`, 'error');
    }
    
    this.log('✅ Renderer Daemon started with modular architecture and widget system');
  }

  public async registerRoutesWithWebSocket(): Promise<void> {
    const WebSocket = (await import('ws')).default;
    
    return new Promise((resolve, reject) => {
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

      ws.on('error', (error: Error) => {
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
      
      const templatePath = request.data.templatePath as string | undefined;
      const html = await this.htmlEngine.renderMainUI({
        version,
        ...(templatePath && { templatePath })
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

  private async handleHttpRequest(data: { pathname: string; handler: string }): Promise<DaemonResponse> {
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

        case 'serve_ui_component': {
          // Serve widget files dynamically using widget system
          const pathParts = pathname.split('/');
          const widgetName = pathParts[pathParts.length - 2]; // Extract widget name from path
          const fileName = pathParts[pathParts.length - 1];
          
          try {
            if (fileName.endsWith('.js')) {
              // Serve compiled JavaScript
              const script = await widgetManager.getWidgetScript(widgetName);
              return {
                success: true,
                data: {
                  contentType: 'application/javascript',
                  content: script,
                  headers: { 'Cache-Control': 'no-cache, no-store, must-revalidate' }
                }
              };
            } else if (fileName.endsWith('.css')) {
              // Serve CSS styles
              const styles = await widgetManager.getWidgetStyles(widgetName);
              return {
                success: true,
                data: {
                  contentType: 'text/css',
                  content: styles,
                  headers: { 'Cache-Control': 'no-cache, no-store, must-revalidate' }
                }
              };
            } else {
              // Serve other assets
              const fs2 = await import('fs');
              const path2 = await import('path');
              const { fileURLToPath: fileURLToPath2 } = await import('url');
              
              const __dirname2 = path2.dirname(fileURLToPath2(import.meta.url));
              const relativePath = pathname.replace(/^\/src\/ui\//, '');
              const componentPath = path2.join(__dirname2, '../../ui/', relativePath);
              
              if (fs2.existsSync(componentPath)) {
                const content = fs2.readFileSync(componentPath, 'utf-8');
                return {
                  success: true,
                  data: {
                    contentType: this.getContentType(fileName),
                    content,
                    headers: { 'Cache-Control': 'no-cache, no-store, must-revalidate' }
                  }
                };
              }
            }
          } catch (error) {
            console.error(`❌ Error serving widget component ${pathname}:`, error);
            return {
              success: false,
              error: `Failed to serve widget component: ${pathname}`
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

  // Widget system handlers
  private async handleRenderWidget(data: { name: string }): Promise<DaemonResponse> {
    try {
      const html = await widgetManager.renderWidget(data.name);
      return {
        success: true,
        data: { html }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to render widget ${data.name}: ${errorMessage}`
      };
    }
  }

  private async handleGetWidgetAssets(data: { name: string }): Promise<DaemonResponse> {
    try {
      const assets = await widgetManager.getWidgetAssets(data.name);
      return {
        success: true,
        data: { assets }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to get widget assets for ${data.name}: ${errorMessage}`
      };
    }
  }

  private async handleGetWidgetScript(data: { name: string }): Promise<DaemonResponse> {
    try {
      const script = await widgetManager.getWidgetScript(data.name);
      return {
        success: true,
        data: { script }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to get widget script for ${data.name}: ${errorMessage}`
      };
    }
  }

  private async handleGetWidgetStyles(data: { name: string }): Promise<DaemonResponse> {
    try {
      const styles = await widgetManager.getWidgetStyles(data.name);
      return {
        success: true,
        data: { styles }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to get widget styles for ${data.name}: ${errorMessage}`
      };
    }
  }

  private getContentType(filename: string): string {
    const ext = filename.split('.').pop()?.toLowerCase();
    switch (ext) {
      case 'js': return 'application/javascript';
      case 'css': return 'text/css';
      case 'html': return 'text/html';
      case 'json': return 'application/json';
      case 'png': return 'image/png';
      case 'jpg': case 'jpeg': return 'image/jpeg';
      case 'svg': return 'image/svg+xml';
      default: return 'text/plain';
    }
  }

  /**
   * DEPRECATED: Use WebSocket message-based registration instead
   */
  public registerWithWebSocketDaemon(_webSocketDaemon: unknown): void {
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