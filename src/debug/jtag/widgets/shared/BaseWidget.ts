/**
 * BaseWidget - Abstract all complexity away from widget implementations
 * 
 * Just like JTAG commands abstract away server/browser coordination,
 * this BaseWidget abstracts away:
 * - Daemon communication (database, router, academy, etc.)
 * - Theme system integration
 * - Event coordination  
 * - State persistence
 * - Error handling
 * - Performance optimization
 * 
 * Subclasses get powerful one-line operations:
 * - this.storeData(key, value) - handles database + caching
 * - this.broadcastEvent(type, data) - handles router + WebSocket
 * - this.notifyAI(message) - handles Academy daemon
 * - this.applyTheme(name) - handles theme system
 */

import { 
  ChatEventEmitter,
  ChatEventData,
  ChatEventType 
} from '../chat-widget/shared/ChatTypes';
import type { FileLoadParams, FileLoadResult } from '../../commands/file/load/shared/FileLoadTypes';
import type { FileSaveParams, FileSaveResult } from '../../commands/file/save/shared/FileSaveTypes';
import type { ScreenshotParams, ScreenshotResult } from '../../commands/screenshot/shared/ScreenshotTypes';

export interface WidgetConfig {
  // Core settings
  widgetId: string;
  widgetName: string;
  version?: string;
  
  // Theme and appearance
  theme?: 'basic' | 'cyberpunk' | 'anime' | 'custom';
  customTheme?: Record<string, string>;
  compactMode?: boolean;
  
  // Resource files
  template?: string;  // HTML template filename
  styles?: string;    // CSS styles filename
  
  // Data and persistence
  enablePersistence?: boolean;
  cacheData?: boolean;
  syncAcrossDevices?: boolean;
  
  // Integration settings
  enableAI?: boolean;
  enableDatabase?: boolean;
  enableRouterEvents?: boolean;
  enableScreenshots?: boolean;
  
  // Debug and development
  debugMode?: boolean;
  visualDebugging?: boolean;
  performanceMonitoring?: boolean;
}

export interface WidgetState {
  isInitialized: boolean;
  isConnected: boolean;
  hasError: boolean;
  errorMessage?: string;
  lastUpdate: string;
  theme: string;
  data: Map<string, any>;
  cache: Map<string, any>;
}

export interface WidgetContext {
  environment: 'browser' | 'server' | 'shared';
  sessionId: string;
  userId: string;
  permissions: string[];
  capabilities: string[];
}

export abstract class BaseWidget extends HTMLElement {
  declare shadowRoot: ShadowRoot;
  protected eventEmitter = new ChatEventEmitter();
  protected config: WidgetConfig;
  protected state: WidgetState;
  protected context: WidgetContext;
  
  // Daemon connections (abstracted away from subclasses)
  private databaseDaemon?: any;
  private routerDaemon?: any;
  private academyDaemon?: any;
  private themeDaemon?: any;
  
  // Performance and caching
  private operationCache = new Map<string, any>();
  private throttledOperations = new Map<string, number>();
  
  // Template resources (loaded from external files)
  protected templateHTML?: string;
  protected templateCSS?: string;
  
  constructor(config: Partial<WidgetConfig> = {}) {
    super();
    this.attachShadow({ mode: 'open' });
    
    // Default configuration with smart defaults
    this.config = {
      widgetId: this.generateWidgetId(),
      widgetName: this.constructor.name,
      version: '1.0.0',
      theme: 'cyberpunk',
      enablePersistence: true,
      cacheData: true,
      enableAI: true,
      enableDatabase: true,
      enableRouterEvents: true,
      enableScreenshots: true,
      debugMode: false,
      visualDebugging: false,
      performanceMonitoring: true,
      ...config
    };
    
    // Initialize state
    this.state = {
      isInitialized: false,
      isConnected: false,
      hasError: false,
      lastUpdate: new Date().toISOString(),
      theme: this.config.theme || 'cyberpunk',
      data: new Map(),
      cache: new Map()
    };
    
    // Initialize context (would be populated by widget system)
    this.context = {
      environment: 'browser', // Detected automatically
      sessionId: this.generateSessionId(),
      userId: 'current_user', // From user system
      permissions: ['read', 'write'], // From permission system
      capabilities: ['screenshot', 'file/save', 'ai_integration'] // From capability system
    };
  }

  async connectedCallback(): Promise<void> {
    try {
      console.log(`🎨 ${this.config.widgetName}: BaseWidget initialization starting...`);
      
      // 1. Connect to daemon systems (abstracted)
      await this.initializeDaemonConnections();
      
      // 2. Load and apply theme (abstracted)
      await this.initializeTheme();
      
      // 3. Restore persisted state (abstracted)
      await this.restorePersistedState();
      
      // 4. Load external resources (template & styles)
      await this.loadResources();
      
      // 5. Let subclass initialize its specific logic
      await this.onWidgetInitialize();
      
      // 6. Render UI (subclass-specific but with base support)
      await this.renderWidget();
      
      // 7. Setup event coordination (abstracted)
      await this.initializeEventSystem();
      
      this.state.isInitialized = true;
      this.state.isConnected = true;
      
      console.log(`✅ ${this.config.widgetName}: BaseWidget initialization complete`);
      
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: Initialization failed:`, error);
      this.handleInitializationError(error);
    }
  }

  async disconnectedCallback(): Promise<void> {
    console.log(`🎨 ${this.config.widgetName}: BaseWidget cleanup starting...`);
    
    // Let subclass clean up first
    await this.onWidgetCleanup();
    
    // Persist final state
    await this.persistCurrentState();
    
    // Disconnect from daemons
    this.disconnectFromDaemons();
    
    // Clean up event listeners
    this.eventEmitter.clear();
    this.operationCache.clear();
    this.throttledOperations.clear();
    
    this.state.isConnected = false;
    console.log(`✅ ${this.config.widgetName}: BaseWidget cleanup complete`);
  }

  // === ABSTRACT METHODS - Subclasses must implement ===
  
  /**
   * Subclass-specific initialization
   * Called after daemon connections are established
   */
  protected abstract onWidgetInitialize(): Promise<void>;
  
  /**
   * Render the widget UI
   * BaseWidget provides theme, but subclass provides content
   */
  protected abstract renderWidget(): Promise<void>;
  
  /**
   * Subclass-specific cleanup
   * Called before daemon disconnection
   */
  protected abstract onWidgetCleanup(): Promise<void>;

  // === ABSTRACTED OPERATIONS - One-line power for subclasses ===

  /**
   * Store data with automatic database + cache coordination
   * Like JTAG's this.screenshot() - one call, full coordination
   */
  protected async storeData(key: string, value: any, options: { 
    persistent?: boolean;
    broadcast?: boolean;
    ttl?: number;
  } = {}): Promise<boolean> {
    try {
      const {
        persistent = this.config.enablePersistence,
        broadcast = this.config.enableRouterEvents,
        ttl = 3600000 // 1 hour default
      } = options;
      
      // 1. Update local cache immediately
      this.state.cache.set(key, { value, timestamp: Date.now(), ttl });
      
      // 2. Store in database if enabled
      if (persistent && this.config.enableDatabase) {
        await this.databaseOperation('store', {
          widgetId: this.config.widgetId,
          key,
          value: JSON.stringify(value),
          ttl
        });
      }
      
      // 3. Broadcast change if enabled
      if (broadcast) {
        await this.broadcastEvent('data_updated', { key, value });
      }
      
      // 4. Update widget state
      this.state.data.set(key, value);
      this.state.lastUpdate = new Date().toISOString();
      
      return true;
      
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: storeData failed for key ${key}:`, error);
      return false;
    }
  }

  /**
   * Retrieve data with automatic cache + database coordination
   */
  protected async getData(key: string, defaultValue?: any): Promise<any> {
    try {
      // 1. Check cache first (fastest)
      const cached = this.state.cache.get(key);
      if (cached && this.isCacheValid(cached)) {
        return cached.value;
      }
      
      // 2. Check widget state
      if (this.state.data.has(key)) {
        return this.state.data.get(key);
      }
      
      // 3. Load from database if enabled
      if (this.config.enableDatabase) {
        const dbResult = await this.databaseOperation('retrieve', {
          widgetId: this.config.widgetId,
          key
        });
        
        if (dbResult.success && dbResult.data.value) {
          const value = JSON.parse(dbResult.data.value);
          
          // Update cache and state
          this.state.cache.set(key, { value, timestamp: Date.now() });
          this.state.data.set(key, value);
          
          return value;
        }
      }
      
      return defaultValue;
      
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: getData failed for key ${key}:`, error);
      return defaultValue;
    }
  }

  /**
   * Broadcast event with automatic router + WebSocket coordination
   * Like JTAG's cross-environment messaging but for widgets
   */
  protected async broadcastEvent(eventType: string, data: any, options: {
    targetWidgets?: string[];
    excludeSelf?: boolean;
    persistent?: boolean;
  } = {}): Promise<boolean> {
    try {
      if (!this.config.enableRouterEvents) return false;
      
      const {
        targetWidgets,
        excludeSelf = true,
        persistent = false
      } = options;
      
      const eventData = {
        sourceWidget: this.config.widgetId,
        eventType,
        data,
        timestamp: new Date().toISOString(),
        targetWidgets,
        excludeSelf
      };
      
      // Send via router daemon (handles WebSocket, cross-browser, etc.)
      const result = await this.routerOperation('broadcast', {
        channel: 'widget_events',
        event: eventData,
        persistent
      });
      
      return result.success;
      
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: broadcastEvent failed:`, error);
      return false;
    }
  }

  /**
   * AI integration with automatic Academy daemon coordination
   */
  protected async queryAI(message: string, options: {
    persona?: string;
    context?: any;
    expectResponse?: boolean;
  } = {}): Promise<any> {
    try {
      if (!this.config.enableAI) {
        console.warn(`⚠️ ${this.config.widgetName}: AI disabled for this widget`);
        return null;
      }
      
      const {
        persona = 'general_assistant',
        context = this.getAIContext(),
        expectResponse = true
      } = options;
      
      const result = await this.academyOperation('query', {
        message,
        persona,
        context,
        widgetId: this.config.widgetId,
        expectResponse
      });
      
      return result.success ? result.data : null;
      
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: queryAI failed:`, error);
      return null;
    }
  }

  /**
   * Screenshot widget with automatic JTAG coordination
   * Just like base JTAG screenshot command
   */
  protected async takeScreenshot(options: {
    filename?: string;
    selector?: string;
    includeContext?: boolean;
  } = {}): Promise<string | null> {
    try {
      if (!this.config.enableScreenshots) return null;
      
      const {
        filename = `${this.config.widgetName}-${Date.now()}.png`,
        selector = `:host`, // Screenshot this widget by default
        includeContext = true
      } = options;
      
      // Use JTAG screenshot command with proper types
      const result = await this.jtagOperation<ScreenshotResult>('screenshot', {
        filename,
        querySelector: selector,
        includeContext
      });
      
      return result ? result.filepath : null;
      
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: takeScreenshot failed:`, error);
      return null;
    }
  }

  /**
   * Apply theme with automatic theme system coordination
   */
  protected async applyTheme(themeName: string, customProperties?: Record<string, string>): Promise<boolean> {
    try {
      console.log(`🎨 ${this.config.widgetName}: Applying theme '${themeName}'...`);
      
      // 1. Load theme CSS file directly (themes are in shared directory, not widget-specific)
      const themePath = `widgets/shared/themes/${themeName}.css`;
      console.log(`🎨 ${this.config.widgetName}: Loading theme from ${themePath}`);
      
      const result = await this.jtagOperation<FileLoadResult>('file/load', {
        filepath: themePath
      });
      
      // Handle nested JTAG response structure - actual data is in commandResult
      const fileData = (result as any).commandResult || result;
      let themeCSS: string;
      
      if (result.success && fileData.success && fileData.content) {
        console.log(`✅ ${this.config.widgetName}: Theme loaded successfully (${fileData.bytesRead} bytes)`);
        themeCSS = fileData.content;
      } else {
        console.warn(`⚠️ ${this.config.widgetName}: Theme load failed: ${themePath}`);
        console.warn(`  Debug: result.success=${result.success}, fileData.success=${fileData.success}, has content=${!!fileData.content}`);
        themeCSS = '/* No theme loaded */';
      }
      
      if (themeCSS === '/* No theme loaded */') {
        console.warn(`⚠️ ${this.config.widgetName}: Theme '${themeName}' not found, using default`);
        return false;
      }
      
      // 2. Inject theme CSS into document head (site-wide theming)
      this.injectThemeIntoDocument(themeName, themeCSS);
      
      // 3. Apply any custom properties if provided
      if (customProperties) {
        this.applyCustomCSSProperties(customProperties);
      }
      
      // 4. Update state and persist preference
      this.state.theme = themeName;
      await this.storeData('current_theme', themeName, { persistent: true });
      
      // 5. Notify other widgets about theme change
      this.eventEmitter.emit('theme_changed', { 
        themeName, 
        widgetId: this.config.widgetId,
        customProperties
      });
      
      console.log(`✅ ${this.config.widgetName}: Theme '${themeName}' applied successfully`);
      return true;
      
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: applyTheme failed:`, error);
      return false;
    }
  }

  /**
   * Save file with automatic file system coordination
   */
  protected async saveFile(filename: string, content: string | Blob, options: {
    directory?: string;
    format?: string;
    compress?: boolean;
  } = {}): Promise<string | null> {
    try {
      const {
        directory = 'widget_data',
        format = 'auto',
        compress = false
      } = options;
      
      // Use JTAG file/save command with proper types
      const result = await this.jtagOperation<FileSaveResult>('file/save', {
        filepath: `${directory}/${filename}`,
        content: content,
        createDirs: true
      });
      
      return result ? result.filepath : null;
      
    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: saveFile failed:`, error);
      return null;
    }
  }

  // === OVERRIDE POINTS - Subclasses can customize these ===

  /**
   * Override to customize AI context sent with queries
   */
  protected getAIContext(): any {
    return {
      widgetType: this.config.widgetName,
      currentState: Object.fromEntries(this.state.data),
      theme: this.state.theme,
      capabilities: this.context.capabilities
    };
  }

  /**
   * Override to customize error handling
   */
  protected handleError(error: any, operation: string): void {
    console.error(`❌ ${this.config.widgetName}: ${operation} error:`, error);
    this.state.hasError = true;
    this.state.errorMessage = error.message || String(error);
    
    // Default error UI - subclasses can override
    this.showErrorMessage(`${operation} failed: ${error.message}`);
  }

  /**
   * Override to customize performance monitoring
   */
  protected logPerformance(operation: string, startTime: number): void {
    if (!this.config.performanceMonitoring) return;
    
    const duration = performance.now() - startTime;
    console.log(`⚡ ${this.config.widgetName}: ${operation} took ${duration.toFixed(2)}ms`);
  }

  /**
   * Override to customize event handling
   */
  protected onEventReceived(eventType: string, data: any): void {
    // Default: just emit to internal event system
    this.eventEmitter.emit(eventType as any, data);
  }

  /**
   * Override to customize state serialization
   */
  protected serializeState(): any {
    return {
      data: Object.fromEntries(this.state.data),
      theme: this.state.theme,
      lastUpdate: this.state.lastUpdate,
      config: this.config
    };
  }

  /**
   * Override to customize state deserialization
   */
  protected deserializeState(serialized: any): void {
    if (serialized.data) {
      this.state.data = new Map(Object.entries(serialized.data));
    }
    if (serialized.theme) {
      this.state.theme = serialized.theme;
    }
    if (serialized.config) {
      this.config = { ...this.config, ...serialized.config };
    }
  }

  /**
   * Load external resources (HTML template and CSS styles) via HTTP fetch
   * Uses HTTP server instead of file/load JTAG command to avoid session directory issues
   */
  private async loadResources(): Promise<void> {
    if (!this.config.template && !this.config.styles) {
      return; // No resources to load
    }

    try {
      // Load HTML template if specified
      if (this.config.template) {
        this.templateHTML = await this.loadResource(this.config.template, 'template', '<div>Widget template not found</div>');
      }

      // Load CSS styles if specified  
      if (this.config.styles) {
        this.templateCSS = await this.loadResource(this.config.styles, 'styles', '/* Widget styles not found */');
      }

    } catch (error) {
      console.error(`❌ ${this.config.widgetName}: Resource loading failed:`, error);
      // Provide fallback content
      this.templateHTML = this.templateHTML || '<div>Resource loading error</div>';
      this.templateCSS = this.templateCSS || '/* Fallback styles */';
    }
  }

  /**
   * Resolve resource path for JTAG file/load command
   */
  private resolveResourcePath(filename: string): string {
    // Extract widget directory name from widget name (ChatWidget -> chat)
    const widgetDir = this.config.widgetName.toLowerCase().replace('widget', '');
    // Return relative path from current working directory
    return `widgets/${widgetDir}/public/${filename}`;
  }

  /**
   * Load a single resource using JTAG file/load command
   */
  private async loadResource(filename: string, resourceType: string, fallback: string): Promise<string> {
    const resourcePath = this.resolveResourcePath(filename);
    const emoji = resourceType === 'template' ? '📄' : '🎨';
    
    console.log(`${emoji} ${this.config.widgetName}: Loading ${resourceType} from ${resourcePath}`);
    
    try {
      const result = await this.jtagOperation<FileLoadResult>('file/load', {
        filepath: resourcePath
      });
      
      // Handle nested JTAG response structure - actual data is in commandResult
      const fileData = (result as any).commandResult || result;
      if (result.success && fileData.success && fileData.content) {
        console.log(`✅ ${this.config.widgetName}: ${resourceType} loaded successfully (${fileData.bytesRead} bytes)`);
        return fileData.content;
      } else {
        console.warn(`⚠️ ${this.config.widgetName}: ${resourceType} load failed: ${resourcePath}`);
        console.warn(`  Debug: result.success=${result.success}, fileData.success=${fileData.success}, has content=${!!fileData.content}`);
        return fallback;
      }
    } catch (loadError) {
      console.warn(`⚠️ ${this.config.widgetName}: ${resourceType} load error: ${resourcePath}`, loadError);
      return fallback;
    }
  }

  /**
   * Inject theme CSS into document head for site-wide theming
   */
  private injectThemeIntoDocument(themeName: string, themeCSS: string): void {
    const themeId = `jtag-theme-${themeName}`;
    
    // Remove existing theme if present
    const existingTheme = document.getElementById(themeId);
    if (existingTheme) {
      existingTheme.remove();
    }
    
    // Create and inject new theme style element
    const styleElement = document.createElement('style');
    styleElement.id = themeId;
    styleElement.textContent = themeCSS;
    document.head.appendChild(styleElement);
    
    console.log(`🎨 ${this.config.widgetName}: Theme CSS injected into document head`);
  }

  /**
   * Apply custom CSS properties to document root for site-wide theming
   */
  private applyCustomCSSProperties(customProperties: Record<string, string>): void {
    const documentStyle = document.documentElement.style;
    
    for (const [property, value] of Object.entries(customProperties)) {
      // Ensure property starts with --
      const cssProperty = property.startsWith('--') ? property : `--${property}`;
      documentStyle.setProperty(cssProperty, value);
    }
    
    console.log(`🎨 ${this.config.widgetName}: Applied ${Object.keys(customProperties).length} custom CSS properties`);
  }

  // === PRIVATE IMPLEMENTATION - Hidden complexity ===

  private async initializeDaemonConnections(): Promise<void> {
    if (this.config.enableDatabase) {
      this.databaseDaemon = await this.connectToDaemon('database');
    }
    
    if (this.config.enableRouterEvents) {
      this.routerDaemon = await this.connectToDaemon('router');
    }
    
    if (this.config.enableAI) {
      this.academyDaemon = await this.connectToDaemon('academy');
    }
    
    this.themeDaemon = await this.connectToDaemon('theme');
  }

  private async initializeTheme(): Promise<void> {
    const savedTheme = await this.getData('current_theme', this.config.theme);
    if (savedTheme) {
      await this.applyTheme(savedTheme);
    }
  }

  private async restorePersistedState(): Promise<void> {
    if (!this.config.enablePersistence) return;
    
    const serializedState = await this.getData('widget_state');
    if (serializedState) {
      this.deserializeState(serializedState);
    }
  }

  private async persistCurrentState(): Promise<void> {
    if (!this.config.enablePersistence) return;
    
    const serialized = this.serializeState();
    await this.storeData('widget_state', serialized, { persistent: true });
  }

  private async initializeEventSystem(): Promise<void> {
    if (this.routerDaemon) {
      // Subscribe to widget events
      await this.routerOperation('subscribe', {
        channel: 'widget_events',
        callback: (eventData: any) => {
          if (eventData.sourceWidget !== this.config.widgetId) {
            this.onEventReceived(eventData.eventType, eventData.data);
          }
        }
      });
    }
  }

  private disconnectFromDaemons(): void {
    // Clean disconnection from all daemon systems
    this.databaseDaemon = undefined;
    this.routerDaemon = undefined;
    this.academyDaemon = undefined;
    this.themeDaemon = undefined;
  }

  private async handleInitializationError(error: any): Promise<void> {
    this.state.hasError = true;
    this.state.errorMessage = error.message || String(error);
    
    // Render error state
    this.shadowRoot.innerHTML = `
      <div class="widget-error">
        <div class="error-title">❌ Widget Error</div>
        <div class="error-message">${this.state.errorMessage}</div>
        <button onclick="location.reload()">Reload</button>
      </div>
    `;
  }

  private showErrorMessage(message: string): void {
    // Simple error display - subclasses should override for better UX
    const errorEl = document.createElement('div');
    errorEl.className = 'widget-error-toast';
    errorEl.textContent = message;
    this.shadowRoot.appendChild(errorEl);
    
    setTimeout(() => errorEl.remove(), 5000);
  }

  private applyCSSProperties(styles: Record<string, string>): void {
    const styleElement = this.shadowRoot.querySelector('style') || document.createElement('style');
    
    let css = ':host {\n';
    for (const [property, value] of Object.entries(styles)) {
      css += `  --${property}: ${value};\n`;
    }
    css += '}\n';
    
    styleElement.textContent = (styleElement.textContent || '') + css;
    
    if (!styleElement.parentNode) {
      this.shadowRoot.appendChild(styleElement);
    }
  }

  private isCacheValid(cached: { timestamp: number; ttl?: number }): boolean {
    const ttl = cached.ttl || 3600000; // 1 hour default
    return (Date.now() - cached.timestamp) < ttl;
  }

  // Daemon operation abstractions
  private async databaseOperation(operation: string, data: any): Promise<any> {
    if (!this.databaseDaemon) throw new Error('Database daemon not connected');
    // Would integrate with actual daemon messaging system
    return { success: true, data: {} };
  }

  private async routerOperation(operation: string, data: any): Promise<any> {
    if (!this.routerDaemon) throw new Error('Router daemon not connected');
    // Would integrate with actual daemon messaging system
    return { success: true, data: {} };
  }

  private async academyOperation(operation: string, data: any): Promise<any> {
    if (!this.academyDaemon) throw new Error('Academy daemon not connected');
    // Would integrate with actual daemon messaging system
    return { success: true, data: {} };
  }

  private async themeOperation(operation: string, data: any): Promise<any> {
    if (!this.themeDaemon) throw new Error('Theme daemon not connected');
    // Would integrate with actual daemon messaging system
    return { success: true, data: { styles: {} } };
  }

  // Core JTAG integration - delegates to JTAG system with proper typing
  protected async jtagOperation<T>(command: string, params?: Record<string, any>): Promise<T> {
    try {
      // Wait for JTAG system to be ready using proper events
      await this.waitForSystemReady();
      
      // Get the JTAG client from window
      const jtagClient = (window as any).jtag; // TODO: Add proper window.jtag typing
      if (!jtagClient || !jtagClient.commands) {
        throw new Error('JTAG client not available even after system ready event');
      }
      
      // Execute command through the global JTAG system
      const result = await jtagClient.commands[command](params);
      
      console.log(`🔧 BaseWidget: JTAG operation ${command} completed:`, result);
      return result as T;
      
    } catch (error) {
      console.error(`❌ BaseWidget: JTAG operation ${command} failed:`, error);
      throw error;
    }
  }

  /**
   * Wait for window.jtag to be available (simple polling without timeout complexity)
   */
  private async waitForSystemReady(): Promise<void> {
    return new Promise((resolve) => {
      // Check if system is already ready
      const jtagClient = (window as any).jtag;
      if (jtagClient && jtagClient.commands) {
        console.log(`✅ BaseWidget: JTAG system already ready for ${this.config.widgetName}`);
        resolve();
        return;
      }
      
      console.log(`⏳ BaseWidget: Waiting for JTAG system to be ready for ${this.config.widgetName}`);
      
      // Simple polling - check every 100ms for window.jtag
      const checkReady = () => {
        const jtag = (window as any).jtag;
        if (jtag && jtag.commands) {
          console.log(`✅ BaseWidget: JTAG system ready for ${this.config.widgetName}`);
          resolve();
        } else {
          setTimeout(checkReady, 100);
        }
      };
      
      checkReady();
    });
  }

  private async connectToDaemon(daemonName: string): Promise<any> {
    // Would connect to actual daemon system
    console.log(`🔌 ${this.config.widgetName}: Connected to ${daemonName} daemon`);
    return {};
  }

  private generateWidgetId(): string {
    return `widget_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  private generateSessionId(): string {
    return `session_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }
}