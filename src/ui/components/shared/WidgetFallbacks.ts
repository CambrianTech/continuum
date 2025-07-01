/**
 * Widget Fallbacks - Default HTML for unloaded widgets
 * Shows warning messages and basic structure when widgets aren't loaded
 * Provides client-side console warnings and visible HTML errors
 */

export interface WidgetFallbackConfig {
  tagName: string;
  widgetName: string;
  icon: string;
  description: string;
  expectedFile: string;
  dependencies?: string[];
  customHTML?: (warning?: string) => string;  // Allow custom fallback HTML with optional warning
  customCSS?: () => string;   // Allow custom fallback CSS
}

export class WidgetFallbackElement extends HTMLElement {
  private config: WidgetFallbackConfig;
  private warningMessage?: string;

  constructor(config: WidgetFallbackConfig, warning?: string) {
    super();
    this.config = config;
    if (warning !== undefined) {
      this.warningMessage = warning;
    }
    this.attachShadow({ mode: 'open' });
  }

  connectedCallback() {
    this.renderFallback();
    this.logWarning();
  }

  private renderFallback() {
    // Use custom HTML/CSS if provided, otherwise use default
    const html = this.config.customHTML ? this.config.customHTML(this.warningMessage) : this.generateFallbackHTML();
    const css = this.config.customCSS ? this.config.customCSS() : this.generateFallbackCSS();
    
    if (this.shadowRoot) {
      this.shadowRoot.innerHTML = `
      <style>${css}</style>
      ${html}
    `;
  }

  private generateFallbackHTML(): string {
    const dependencies = this.config.dependencies?.join(', ') || 'continuum-api';
    
    return `
      <div class="widget-fallback">
        <div class="fallback-header">
          <span class="fallback-icon">${this.config.icon}</span>
          <div class="fallback-info">
            <div class="fallback-title">${this.config.widgetName} (Not Loaded)</div>
            <div class="fallback-subtitle">Widget implementation not found</div>
          </div>
          <span class="fallback-status">❌</span>
        </div>
        
        <div class="fallback-content">
          ${this.warningMessage ? `
            <div class="warning-message">
              <p><strong>⚠️ Warning:</strong> ${this.warningMessage}</p>
            </div>
          ` : ''}
          
          <div class="fallback-description">
            ${this.config.description}
          </div>
          
          <div class="fallback-error">
            <strong>Missing:</strong> ${this.config.expectedFile}
          </div>
          
          <div class="fallback-dependencies">
            <strong>Dependencies:</strong> ${dependencies}
          </div>
          
          <div class="fallback-actions">
            <button class="retry-load" onclick="window.location.reload()">
              🔄 Retry Load
            </button>
            <button class="show-console" onclick="console.log('Widget Debug:', this.getRootNode().host)">
              🔍 Debug
            </button>
          </div>
        </div>
      </div>
    `;
  }

  private generateFallbackCSS(): string {
    return `
      .widget-fallback {
        border: 2px dashed #ff6b6b;
        border-radius: 8px;
        padding: 16px;
        margin: 8px;
        background: #2d1b1b;
        color: #ff9999;
        font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
        min-height: 120px;
        display: flex;
        flex-direction: column;
      }
      
      .fallback-header {
        display: flex;
        align-items: center;
        gap: 12px;
        margin-bottom: 12px;
        padding-bottom: 8px;
        border-bottom: 1px solid #ff6b6b33;
      }
      
      .fallback-icon {
        font-size: 24px;
        opacity: 0.7;
      }
      
      .fallback-info {
        flex: 1;
      }
      
      .fallback-title {
        font-weight: bold;
        font-size: 16px;
        color: #ff6b6b;
      }
      
      .fallback-subtitle {
        font-size: 12px;
        opacity: 0.8;
        margin-top: 2px;
      }
      
      .fallback-status {
        font-size: 20px;
      }
      
      .fallback-content {
        flex: 1;
        display: flex;
        flex-direction: column;
        gap: 8px;
      }
      
      .fallback-description {
        font-size: 14px;
        line-height: 1.4;
        opacity: 0.9;
      }
      
      .warning-message {
        margin-bottom: 12px;
        padding: 8px 12px;
        background: #ffa50022;
        border: 1px solid #ffa500;
        border-radius: 6px;
        color: #ff8c00;
      }
      
      .warning-message p {
        margin: 0;
        font-size: 13px;
        line-height: 1.4;
      }
      
      .fallback-error,
      .fallback-dependencies {
        font-size: 12px;
        padding: 6px 8px;
        background: #ff6b6b22;
        border-radius: 4px;
        border-left: 3px solid #ff6b6b;
      }
      
      .fallback-actions {
        display: flex;
        gap: 8px;
        margin-top: 8px;
      }
      
      .fallback-actions button {
        padding: 6px 12px;
        background: #ff6b6b;
        color: white;
        border: none;
        border-radius: 4px;
        cursor: pointer;
        font-size: 12px;
        transition: background 0.2s;
      }
      
      .fallback-actions button:hover {
        background: #ff5252;
      }
      
      @media (max-width: 600px) {
        .fallback-header {
          flex-direction: column;
          align-items: flex-start;
        }
        
        .fallback-actions {
          flex-direction: column;
        }
      }
    `;
  }

  private logWarning() {
    // Check if widget is actually registered as a custom element
    const isWidgetRegistered = customElements.get(this.config.tagName);
    
    if (isWidgetRegistered) {
      // Widget is registered, this is a false positive - don't log warning
      console.log(`✅ Widget "${this.config.tagName}" is properly registered`);
      return;
    }
    
    const message = `⚠️ Widget "${this.config.tagName}" not loaded: ${this.config.expectedFile} missing`;
    const details = {
      tagName: this.config.tagName,
      widgetName: this.config.widgetName,
      expectedFile: this.config.expectedFile,
      dependencies: this.config.dependencies,
      element: this
    };
    
    console.warn(message, details);
    
    // Also log to window for debugging
    if (!(window as any).__CONTINUUM_WIDGET_WARNINGS__) {
      (window as any).__CONTINUUM_WIDGET_WARNINGS__ = [];
    }
    (window as any).__CONTINUUM_WIDGET_WARNINGS__.push({
      timestamp: new Date().toISOString(),
      message,
      ...details
    });
  }
}

export class WidgetFallbackRegistry {
  private static fallbacks: Map<string, WidgetFallbackConfig> = new Map();

  static registerFallback(config: WidgetFallbackConfig) {
    this.fallbacks.set(config.tagName, config);
    
    // Register the fallback custom element if not already defined
    if (!customElements.get(config.tagName)) {
      customElements.define(config.tagName, class extends WidgetFallbackElement {
        constructor() {
          super(config);
        }
      });
      
      console.log(`🔧 Registered fallback for ${config.tagName}`);
    }
  }

  static registerFallbackWithWarning(config: WidgetFallbackConfig, warning: string) {
    this.fallbacks.set(config.tagName, config);
    
    // Register the fallback custom element with warning if not already defined
    if (!customElements.get(config.tagName)) {
      customElements.define(config.tagName, class extends WidgetFallbackElement {
        constructor() {
          super(config, warning);
        }
      });
      
      console.log(`🔧 Registered fallback for ${config.tagName} with warning: ${warning}`);
    }
  }

  static registerAllFallbacks() {
    // Register fallbacks for main widgets
    this.registerFallback({
      tagName: 'chat-widget',
      widgetName: 'ChatWidget',
      icon: '💬',
      description: 'Chat widget for real-time messaging and AI interaction. Handles message display, input, and room management.',
      expectedFile: '/dist/ui/components/Chat/ChatWidget.js',
      dependencies: ['continuum-api', 'websocket-connection']
    });

    this.registerFallback({
      tagName: 'continuum-sidebar',
      widgetName: 'SidebarWidget', 
      icon: '📋',
      description: 'Sidebar widget for navigation and system status. Shows navigation items, connection status, and system info.',
      expectedFile: '/dist/ui/components/Sidebar/SidebarWidget.js',
      dependencies: ['continuum-api'],
      customHTML: (warning?: string) => `
        <div class="sidebar-fallback">
          <div class="sidebar-header">
            <span class="sidebar-icon">📋</span>
            <div class="sidebar-title">Navigation (Not Loaded)</div>
            <span class="error-indicator">❌</span>
          </div>
          
          ${warning ? `
            <div class="warning-banner">
              <p>⚠️ <strong>Warning:</strong> ${warning}</p>
            </div>
          ` : ''}
          
          <div class="sidebar-nav">
            <div class="nav-section">
              <div class="nav-title">📁 Projects</div>
              <div class="nav-item inactive">• Dashboard</div>
              <div class="nav-item inactive">• Current Project</div>
              <div class="nav-item inactive">• Recent Files</div>
            </div>
            
            <div class="nav-section">
              <div class="nav-title">⚙️ System</div>
              <div class="nav-item inactive">• Settings</div>
              <div class="nav-item inactive">• Health Status</div>
              <div class="nav-item inactive">• Connections</div>
            </div>
          </div>
          
          <div class="sidebar-status">
            <div class="status-item">
              <span class="status-label">WebSocket:</span>
              <span class="status-unknown">Unknown</span>
            </div>
            <div class="status-item">
              <span class="status-label">Commands:</span>
              <span class="status-unknown">Unknown</span>
            </div>
          </div>
          
          <div class="error-details">
            <strong>Widget Error:</strong><br>
            SidebarWidget.js not loaded
          </div>
        </div>
      `,
      customCSS: () => `
        .sidebar-fallback {
          background: #2d1b1b;
          border: 2px dashed #ff6b6b;
          border-radius: 8px;
          padding: 16px;
          font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
          color: #ff9999;
          height: 100%;
          display: flex;
          flex-direction: column;
          gap: 16px;
        }
        
        .sidebar-header {
          display: flex;
          align-items: center;
          gap: 8px;
          padding-bottom: 8px;
          border-bottom: 1px solid #ff6b6b33;
        }
        
        .sidebar-icon {
          font-size: 18px;
        }
        
        .sidebar-title {
          flex: 1;
          font-weight: bold;
          font-size: 14px;
        }
        
        .error-indicator {
          font-size: 16px;
        }
        
        .warning-banner {
          margin: 12px 0;
          padding: 8px 12px;
          background: #ffa50022;
          border: 1px solid #ffa500;
          border-radius: 6px;
          color: #ff8c00;
        }
        
        .warning-banner p {
          margin: 0;
          font-size: 12px;
          line-height: 1.4;
        }
        
        .sidebar-nav {
          flex: 1;
          display: flex;
          flex-direction: column;
          gap: 12px;
        }
        
        .nav-section {
          display: flex;
          flex-direction: column;
          gap: 4px;
        }
        
        .nav-title {
          font-weight: bold;
          font-size: 12px;
          color: #ff6b6b;
          margin-bottom: 4px;
        }
        
        .nav-item {
          font-size: 12px;
          padding: 4px 8px;
          border-radius: 3px;
          opacity: 0.6;
        }
        
        .nav-item.inactive {
          color: #ff9999;
          background: #ff6b6b11;
        }
        
        .sidebar-status {
          display: flex;
          flex-direction: column;
          gap: 4px;
          padding: 8px;
          background: #ff6b6b11;
          border-radius: 4px;
          border-left: 3px solid #ff6b6b;
        }
        
        .status-item {
          display: flex;
          justify-content: space-between;
          font-size: 11px;
        }
        
        .status-label {
          opacity: 0.8;
        }
        
        .status-unknown {
          color: #ff6b6b;
          font-weight: bold;
        }
        
        .error-details {
          font-size: 11px;
          padding: 8px;
          background: #ff6b6b22;
          border-radius: 4px;
          border-left: 3px solid #ff6b6b;
          line-height: 1.3;
        }
      `
    });

    this.registerFallback({
      tagName: 'persona-widget',
      widgetName: 'PersonaWidget',
      icon: '🤖',
      description: 'Persona widget for AI persona interaction and management.',
      expectedFile: '/dist/ui/components/Persona/PersonaWidget.js',
      dependencies: ['continuum-api', 'persona-system']
    });

    this.registerFallback({
      tagName: 'version-widget',
      widgetName: 'VersionWidget',
      icon: '📋',
      description: 'Version widget for displaying system version information.',
      expectedFile: '/dist/ui/components/Version/VersionWidget.js',
      dependencies: ['continuum-api']
    });

    this.registerFallback({
      tagName: 'continuon-widget',
      widgetName: 'ContinuonWidget',
      icon: '🌐',
      description: 'Continuon widget for system visualization and control.',
      expectedFile: '/dist/ui/components/Continuon/ContinuonWidget.js', 
      dependencies: ['continuum-api']
    });

    console.log(`🔧 Registered ${this.fallbacks.size} widget fallbacks`);
  }

  static getFallbackConfig(tagName: string): WidgetFallbackConfig | undefined {
    return this.fallbacks.get(tagName);
  }

  static getAllFallbacks(): WidgetFallbackConfig[] {
    return Array.from(this.fallbacks.values());
  }
}

// Auto-register fallbacks when this module loads
WidgetFallbackRegistry.registerAllFallbacks();