/**
 * Renderer Daemon - Encapsulates UI rendering system
 * Currently wraps legacy UIGenerator.cjs, can be swapped for modern TypeScript later
 */

import { BaseDaemon } from '../base/BaseDaemon.js';
import { DaemonMessage, DaemonResponse } from '../base/DaemonProtocol.js';

export interface RenderRequest {
  readonly type: 'render_ui' | 'update_component' | 'render_page';
  readonly data: any;
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
  private dynamicVersion: string = '1.0.0';

  private legacyRenderer: any = null;
  private renderingEngine: 'legacy' | 'modern' = 'legacy';

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
      
      this.log(`✅ Renderer Daemon started with ${this.renderingEngine} engine`);
    } catch (error) {
      this.log(`❌ Failed to start Renderer Daemon: ${error.message}`, 'error');
      this.log(`❌ Stack trace: ${error.stack}`, 'error');
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
      this.log(`⚠️ Failed to load version from package.json: ${error.message}`, 'warn');
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
      this.log(`❌ Render error: ${error.message}`, 'error');
      return {
        success: false,
        error: error.message
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
      this.log(`❌ CRITICAL: Failed to load clean TypeScript UI: ${error.message}`, 'error');
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
      this.legacyRenderer = new UIGeneratorClass(mockContinuum);
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
        // CSS and JS are embedded in the HTML output from legacy renderer
        css: null,
        js: null
      };
      
    } catch (error) {
      this.log(`❌ Legacy renderer failed: ${error.message}`, 'error');
      return {
        success: false,
        error: `Legacy renderer error: ${error.message}`
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

  private getCapabilities(): string[] {
    const capabilities = ['basic-rendering'];
    
    if (this.renderingEngine === 'legacy') {
      capabilities.push('legacy-ui', 'cyberpunk-theme');
    } else {
      capabilities.push('modern-ui', 'typescript-components');
    }
    
    return capabilities;
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