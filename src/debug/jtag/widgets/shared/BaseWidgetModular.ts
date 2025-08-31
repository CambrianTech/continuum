/**
 * BaseWidget - Modular Architecture
 * 
 * Refactored BaseWidget using modular components for better maintainability:
 * - ThemeManager: All theme-related operations
 * - ResourceLoader: External resource loading (HTML, CSS)
 * - DaemonIntegration: JTAG system communication
 * - CSSValidationUtils: Theme validation utilities
 * 
 * Just like JTAG commands abstract away complexity, this BaseWidget provides
 * powerful one-line operations for widgets while maintaining clean separation.
 */

import { ThemeManager, ThemeConfig, ThemeState } from './core/theme/ThemeManager';
import { ResourceLoader, ResourceConfig } from './core/resource/ResourceLoader';
import { DaemonIntegration, DaemonConfig } from './core/daemon/DaemonIntegration';
import { CSSCustomPropertyValidator } from './utils/CSSValidationUtils';

import { 
  ChatEventEmitter,
  ChatEventData,
  ChatEventType 
} from '../chat-widget/shared/ChatTypes';

export interface WidgetConfig {
  // Core settings
  widgetId: string;
  widgetName: string;
  version?: string;
  
  // Theme settings
  theme?: 'basic' | 'cyberpunk' | 'anime' | 'custom';
  customTheme?: Record<string, string>;
  
  // Resource files
  template?: string;
  styles?: string;
  
  // Feature flags
  enableThemeValidation?: boolean;
  enableResourceCaching?: boolean;
  enableDebugging?: boolean;
  enablePersistence?: boolean;
  
  // Development settings
  debugMode?: boolean;
  visualDebugging?: boolean;
}

export interface WidgetState {
  isInitialized: boolean;
  isConnected: boolean;
  hasError: boolean;
  errorMessage?: string;
  lastUpdate: string;
  data: Map<string, any>;
}

export abstract class BaseWidgetModular extends HTMLElement {
  declare shadowRoot: ShadowRoot;
  
  // Core widget properties
  protected eventEmitter = new ChatEventEmitter();
  protected config: WidgetConfig;
  protected state: WidgetState;
  
  // Modular components
  protected themeManager!: ThemeManager;
  protected resourceLoader!: ResourceLoader;
  protected daemonIntegration!: DaemonIntegration;
  
  // Template resources (loaded by ResourceLoader)
  protected templateHTML?: string;
  protected templateCSS?: string;
  
  constructor(config: Partial<WidgetConfig> = {}) {
    super();
    this.attachShadow({ mode: 'open' });
    
    // Set up configuration with smart defaults
    this.config = {
      widgetId: this.generateWidgetId(),
      widgetName: this.constructor.name,
      version: '1.0.0',
      theme: 'cyberpunk',
      enableThemeValidation: true,
      enableResourceCaching: true,
      enableDebugging: false,
      enablePersistence: true,
      debugMode: false,
      visualDebugging: false,
      ...config
    };
    
    // Initialize state
    this.state = {
      isInitialized: false,
      isConnected: false,
      hasError: false,
      lastUpdate: new Date().toISOString(),
      data: new Map()
    };

    // Initialize modular components
    this.initializeModularComponents();
  }

  /**
   * Initialize all modular components with proper configuration
   */
  private initializeModularComponents(): void {
    // Initialize daemon integration
    const daemonConfig: Partial<DaemonConfig> = {
      enableDebugging: this.config.enableDebugging,
      retryAttempts: 3,
      timeoutMs: 10000
    };
    this.daemonIntegration = new DaemonIntegration(this.config.widgetName, daemonConfig);

    // Initialize resource loader
    const resourceConfig: Partial<ResourceConfig> = {
      enableCaching: this.config.enableResourceCaching,
      enableDebugging: this.config.enableDebugging,
      defaultFallbacks: {
        template: '<div>Widget template not found</div>',
        styles: '/* Widget styles not found */'
      }
    };
    const resourceOperations = {
      jtagFileLoad: (filepath: string) => this.daemonIntegration.loadFile(filepath)
    };
    this.resourceLoader = new ResourceLoader(this.config.widgetName, resourceOperations, resourceConfig);

    // Initialize theme manager
    const themeConfig: Partial<ThemeConfig> = {
      defaultTheme: this.config.theme || 'cyberpunk',
      enableValidation: this.config.enableThemeValidation,
      enableDebugging: this.config.enableDebugging,
      enablePersistence: this.config.enablePersistence
    };
    const themeOperations = {
      loadTheme: (themeName: string) => this.resourceLoader.loadTheme(themeName),
      validateTheme: async (themeCSS: string, widgetCSS: string) => {
        const validation = await CSSCustomPropertyValidator.validateProperties(
          widgetCSS, 
          themeCSS,
          {
            enableWarnings: this.config.enableThemeValidation,
            enableDebugging: this.config.enableDebugging,
            widgetName: this.config.widgetName,
            themeName: this.themeManager?.getCurrentTheme() || 'unknown'
          }
        );
        return validation.validationPassed;
      }
    };
    this.themeManager = new ThemeManager(this.config.widgetName, themeConfig, themeOperations);
  }

  async connectedCallback(): Promise<void> {
    try {
      console.log(`🎨 ${this.config.widgetName}: BaseWidget (modular) initialization starting...`);
      
      // 1. Ensure daemon system is ready
      await this.initializeDaemonConnection();
      
      // 2. Load external resources (template & styles)
      await this.loadResources();
      
      // 3. Apply theme with validation
      await this.initializeTheme();
      
      // 4. Restore persisted state
      await this.restorePersistedState();
      
      // 5. Let subclass initialize its specific logic
      await this.onWidgetInitialize();
      
      // 6. Render UI with loaded resources
      await this.renderWidget();
      
      // 7. Setup event system
      await this.initializeEventSystem();
      
      this.state.isInitialized = true;
      this.state.isConnected = true;
      
      console.log(`✅ ${this.config.widgetName}: BaseWidget (modular) initialization complete`);
      
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: Initialization failed:`, error);
      this.handleInitializationError(error);
    }
  }

  async disconnectedCallback(): Promise<void> {
    console.log(`🎨 ${this.config.widgetName}: BaseWidget (modular) cleanup starting...`);
    
    // Let subclass clean up first
    await this.onWidgetCleanup();
    
    // Persist final state
    await this.persistCurrentState();
    
    // Clean up modular components
    this.themeManager.clearTheme();
    this.resourceLoader.clearCache();
    
    // Clean up event listeners and maps
    this.eventEmitter.clear();
    this.state.data.clear();
    
    this.state.isConnected = false;
    console.log(`✅ ${this.config.widgetName}: BaseWidget (modular) cleanup complete`);
  }

  // === ABSTRACT METHODS - Subclasses must implement ===
  
  protected abstract onWidgetInitialize(): Promise<void>;
  protected abstract renderWidget(): Promise<void>;
  protected abstract onWidgetCleanup(): Promise<void>;

  // === POWERFUL ONE-LINE OPERATIONS FOR SUBCLASSES ===

  /**
   * Apply theme with full validation and injection
   */
  protected async applyTheme(themeName: string, customProperties?: Record<string, string>): Promise<boolean> {
    return this.themeManager.applyTheme(themeName, customProperties);
  }

  /**
   * Take screenshot of widget or specific element
   */
  protected async takeScreenshot(options: {
    filename?: string;
    selector?: string;
    includeContext?: boolean;
  } = {}): Promise<string | null> {
    try {
      const {
        filename = `${this.config.widgetName}-${Date.now()}.png`,
        selector = `:host`,
        includeContext = true
      } = options;
      
      const result = await this.daemonIntegration.takeScreenshot(filename, selector, includeContext);
      return result ? result.filepath : null;
      
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: takeScreenshot failed:`, error);
      return null;
    }
  }

  /**
   * Save file with automatic daemon coordination
   */
  protected async saveFile(filename: string, content: string | Blob, directory: string = 'widget_data'): Promise<string | null> {
    try {
      const filepath = `${directory}/${filename}`;
      const result = await this.daemonIntegration.saveFile(filepath, content, true);
      return result ? result.filepath : null;
      
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: saveFile failed:`, error);
      return null;
    }
  }

  /**
   * Store data with automatic persistence
   */
  protected async storeData(key: string, value: any): Promise<boolean> {
    try {
      this.state.data.set(key, value);
      this.state.lastUpdate = new Date().toISOString();
      
      // TODO: Integrate with actual persistence system
      if (this.config.enablePersistence) {
        console.log(`💾 ${this.config.widgetName}: Data stored: ${key} (persistence pending integration)`);
      }
      
      return true;
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: storeData failed for key ${key}:`, error);
      return false;
    }
  }

  /**
   * Retrieve stored data
   */
  protected async getData(key: string, defaultValue?: any): Promise<any> {
    return this.state.data.get(key) ?? defaultValue;
  }

  /**
   * Broadcast event to other widgets
   */
  protected async broadcastEvent(eventType: string, data: any): Promise<void> {
    this.eventEmitter.emit(eventType as any, {
      sourceWidget: this.config.widgetId,
      eventType,
      data,
      timestamp: new Date().toISOString()
    });
  }

  // === PRIVATE IMPLEMENTATION ===

  private async initializeDaemonConnection(): Promise<void> {
    // Daemon integration handles the connection automatically
    console.log(`🔌 ${this.config.widgetName}: JTAG daemon integration ready`);
  }

  private async loadResources(): Promise<void> {
    if (!this.config.template && !this.config.styles) {
      return;
    }

    try {
      if (this.config.template) {
        this.templateHTML = await this.resourceLoader.loadTemplate(this.config.template);
      }

      if (this.config.styles) {
        this.templateCSS = await this.resourceLoader.loadStyles(this.config.styles);
      }

    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: Resource loading failed:`, error);
      this.templateHTML = this.templateHTML || '<div>Resource loading error</div>';
      this.templateCSS = this.templateCSS || '/* Fallback styles */';
    }
  }

  private async initializeTheme(): Promise<void> {
    const savedTheme = await this.getData('current_theme', this.config.theme);
    if (savedTheme) {
      await this.applyTheme(savedTheme);
    }
  }

  private async restorePersistedState(): Promise<void> {
    if (!this.config.enablePersistence) return;
    
    // TODO: Integrate with actual persistence system
    console.log(`💾 ${this.config.widgetName}: State restoration pending integration`);
  }

  private async persistCurrentState(): Promise<void> {
    if (!this.config.enablePersistence) return;
    
    // TODO: Integrate with actual persistence system
    console.log(`💾 ${this.config.widgetName}: State persistence pending integration`);
  }

  private async initializeEventSystem(): Promise<void> {
    // TODO: Integrate with router daemon for cross-widget events
    console.log(`📡 ${this.config.widgetName}: Event system pending integration`);
  }

  private async handleInitializationError(error: any): Promise<void> {
    this.state.hasError = true;
    this.state.errorMessage = error.message || String(error);
    
    this.shadowRoot.innerHTML = `
      <div style="padding: 20px; border: 2px solid #ff4444; border-radius: 8px; background: #ffeeee; color: #cc0000;">
        <div style="font-weight: bold; margin-bottom: 8px;">❌ Widget Error</div>
        <div style="margin-bottom: 12px;">${this.state.errorMessage}</div>
        <button onclick="location.reload()" style="padding: 8px 16px; background: #cc0000; color: white; border: none; border-radius: 4px; cursor: pointer;">Reload</button>
      </div>
    `;
  }

  private generateWidgetId(): string {
    return `widget_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  // === PUBLIC API FOR DEBUGGING ===

  /**
   * Get widget state for debugging
   */
  getWidgetState(): WidgetState & { themeState: ThemeState } {
    return {
      ...this.state,
      themeState: this.themeManager.getThemeState()
    };
  }

  /**
   * Get component statistics for debugging
   */
  getComponentStats(): {
    daemon: any;
    resource: any;
    theme: any;
  } {
    return {
      daemon: this.daemonIntegration.getStats(),
      resource: this.resourceLoader.getCacheStats(),
      theme: this.themeManager.getThemeState()
    };
  }
}