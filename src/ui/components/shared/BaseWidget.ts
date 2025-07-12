/**
 * BaseWidget - Strongly Typed Abstract Base Class for All Widgets
 * Enforces proper implementation through abstract methods and properties
 */

import { globalErrorHandler, captureError } from '../../continuum-browser-client/error/GlobalErrorHandler';
import {
  WidgetDataRequest,
  WidgetDataResponse,
  WidgetCommandRequest,
  WidgetCommandResponse,
  SessionCreatedEvent,
  SessionJoinedEvent,
  HealthUpdatedEvent,
  DataUpdatedEvent,
  DataSourceType,
  WidgetCapabilities
} from '../../../types/shared/WidgetServerTypes';

// Smart asset manifest (zero 404s!) - globally available via esbuild plugin
declare global {
  interface Window {
    WIDGET_ASSETS: Record<string, {css: string[], html: string[], js: string[], directoryName?: string}>;
  }
}


export abstract class BaseWidget extends HTMLElement {
  declare shadowRoot: ShadowRoot;
  
  // Properties that subclasses can set in constructor
  protected widgetName: string = 'BaseWidget';
  protected widgetIcon: string = '🔹';
  protected widgetTitle: string = 'Widget';

  /**
   * Widget name for registration - override in subclasses
   */
  static get widgetName(): string {
    return 'base-widget';
  }

  /**
   * HTML tag name - defaults to widgetName, override only if different
   */
  static get tagName(): string {
    return this.widgetName;
  }
  
  // Protected state
  protected widgetConnected: boolean = false;
  protected isCollapsed: boolean = false;

  // Smart defaults - minimal requirements from subclasses
  
  /**
   * Widget base path - automagic from build-time directory mapping
   */
  public static get basePath(): string {
    const widgetName = this.name.replace(/^_/, '');
    
    // Check if we have build-time directory mapping available
    if (typeof window !== 'undefined' && window.WIDGET_ASSETS) {
      const widgetInfo = window.WIDGET_ASSETS[widgetName];
      if (widgetInfo && widgetInfo.directoryName) {
        return `/src/ui/components/${widgetInfo.directoryName}`;
      }
    }
    
    // Fallback: try to map common class name patterns to directory names
    const directoryMap: Record<string, string> = {
      'ChatWidget': 'Chat',
      'SidebarWidget': 'Sidebar', 
      'PersonaWidget': 'Persona',
      'VersionWidget': 'Version',
      'ContinuonWidget': 'Continuon',
      'ActiveProjectsWidget': 'ActiveProjects',
      'SavedPersonasWidget': 'SavedPersonas',
      'UserSelectorWidget': 'UserSelector'
    };
    
    const directoryName = directoryMap[widgetName] || widgetName;
    return `/src/ui/components/${directoryName}`;
  }
  
  // CSS and HTML loading is now handled directly in loadCSS() and loadHTMLTemplatesOrFallback()
  // using package.json files array - no complex discovery methods needed
  
  /**
   * Get widget files from package.json - replaces manual CSS/HTML declarations
   * Reads the 'files' array from widget's package.json automatically
   */
  static async getWidgetFiles(): Promise<string[]> {
    try {
      const packagePath = `${this.basePath}/package.json`;
      
      console.log(`📦 ${this.name}: Fetching package.json from ${packagePath}`);
      const response = await fetch(packagePath);
      if (!response.ok) {
        console.warn(`📦 No package.json found for ${this.name} at ${packagePath}`);
        return [];
      }
      
      const packageData = await response.json();
      return packageData.files || [];
    } catch (error) {
      console.warn(`📦 Failed to read package.json for ${this.name}:`, error);
      return [];
    }
  }
  
  /**
   * Get all widget assets (except .ts files) - reads from package.json
   * Simple route: widget path + whatever package.json declares
   */
  static async getWidgetAssets(): Promise<string[]> {
    const widgetFiles = await this.getWidgetFiles();
    const assets = widgetFiles.filter(file => !file.endsWith('.ts')); // Serve everything except TypeScript
    return assets.map(file => `${this.basePath}/${file}`);
  }

  
  /**
   * Auto-load HTML templates or fallback to renderContent() - zero burden
   */
  protected async loadHTMLTemplatesOrFallback(): Promise<string> {
    try {
      const constructor = this.constructor as typeof BaseWidget;
      const basePath = constructor.basePath;
      
      // SMART MANIFEST: Only load HTML files that actually exist (Zero 404s!)
      const widgetAssets = window.WIDGET_ASSETS?.[constructor.name.replace(/^_/, '')];
      
      if (widgetAssets && widgetAssets.html.length > 0) {
        console.log(`📁 ${constructor.name}: Found ${widgetAssets.html.length} HTML files in manifest`);
        
        // Try to load HTML files from manifest
        for (const htmlFile of widgetAssets.html) {
          const htmlPath = `${basePath}/${htmlFile}`;
          try {
            const response = await fetch(htmlPath);
            if (response.ok) {
              const htmlContent = await response.text();
              console.log(`✅ Loaded HTML template: ${htmlPath} (Zero 404s!)`);
              return htmlContent;
            } else {
              console.error(`🚨 MANIFEST ERROR: ${htmlPath} not found but was in manifest!`);
            }
          } catch (error) {
            console.error(`🚨 MANIFEST ERROR: Failed to fetch ${htmlPath}:`, error);
          }
        }
      } else {
        console.log(`📁 ${constructor.name}: No HTML files in manifest - using renderContent() fallback`);
      }
      
      // No HTML template found or available, use code-based renderContent()
      return this.renderContent();
    } catch (error) {
      console.warn(`🎨 ${this.widgetName}: HTML template loading failed, using fallback:`, error);
      return this.renderContent();
    }
  }

  /**
   * Load HTML templates if widget declares any (legacy method)
   */
  protected async loadHTMLTemplates(): Promise<string> {
    const constructor = this.constructor as typeof BaseWidget;
    const assets = await constructor.getWidgetAssets();
    const htmlFiles = assets.filter(asset => asset.endsWith('.html'));
    
    if (htmlFiles.length === 0) {
      return this.renderOwnContent(); // Fallback to code-based content
    }
    
    try {
      const constructor = this.constructor as typeof BaseWidget;
      const htmlPromises = htmlFiles.map(file => 
        fetch(`${constructor.basePath}/${file}`).then(r => r.text())
      );
      
      const htmlContents = await Promise.all(htmlPromises);
      return htmlContents.join('\n');
      
    } catch (error) {
      console.warn(`Failed to load HTML templates for ${constructor.name}:`, error);
      return this.renderOwnContent(); // Fallback to code-based content
    }
  }

  /**
   * Widget declares its own HTML content (fallback when no HTML files)
   * Override in child classes to specify widget-specific content
   */
  protected renderOwnContent(): string {
    return '<p>Base widget - override renderOwnContent() or declare getOwnHTML()</p>';
  }
  
  /**
   * Base widget HTML structure - includes collapse, header, content
   */
  protected renderBaseHTML(): string {
    return `
      <div class="widget-container">
        <div class="widget-header" data-action="toggle-collapse">
          <div class="widget-title-row">
            <span class="widget-icon">${this.widgetIcon}</span>
            <span class="widget-title">${this.widgetTitle}</span>
            <span class="collapse-toggle">${this.isCollapsed ? '▶' : '▼'}</span>
          </div>
        </div>
        <div class="widget-content ${this.isCollapsed ? 'collapsed' : ''}">
          ${this.renderOwnContent()}
        </div>
      </div>
    `;
  }
  
  /**
   * Get widget-relative asset path
   */
  protected getAssetPath(relativePath: string): string {
    const basePath = (this.constructor as typeof BaseWidget).basePath;
    return `${basePath}/${relativePath}`;
  }

  constructor() {
    super();
    console.log(`🏗️ ${this.constructor.name}: Constructor called`);
    
    try {
      this.attachShadow({ mode: 'open' });
      console.log(`🏗️ ${this.constructor.name}: Shadow DOM attached`);
    } catch (error) {
      captureError(error, {
        component: 'widget',
        widget: this.constructor.name,
        operation: 'constructor'
      });
      throw error; // Re-throw so widget fails to initialize properly
    }
  }

  async connectedCallback() {
    console.log(`🎛️ ${this.widgetName}: connectedCallback() triggered - connecting to DOM`);
    this.widgetConnected = true;
    
    // Wrap with global error handler that captures EVERYTHING
    await globalErrorHandler.safeExecute(async () => {
      console.log(`🎛️ ${this.widgetName}: About to call initializeWidget()`);
      await this.initializeWidget();
      console.log(`🎛️ ${this.widgetName}: About to call render()`);
      await this.render();
      console.log(`🎛️ ${this.widgetName}: connectedCallback() complete`);
    }, {
      component: 'widget',
      widget: this.widgetName,
      operation: 'connectedCallback'
    });
    
    // If initialization failed, render basic fallback (avoid infinite loops)
    if (!this.shadowRoot?.hasChildNodes()) {
      // Simple fallback without console calls that could trigger infinite loops
      if (this.shadowRoot) {
        this.shadowRoot.innerHTML = `<div style="padding: 8px; color: #666; font-size: 12px;">⚠️ ${this.widgetName}: Loading...</div>`;
      }
    }
  }

  /**
   * Initialize widget - override for custom initialization
   */
  protected async initializeWidget(): Promise<void> {
    // Setup server event listeners
    this.setupServerEventListeners();
    
    // Override in child classes for custom initialization
  }

  /**
   * Setup server event listeners - automatically called during initialization
   */
  private setupServerEventListeners(): void {
    // Listen for server events
    this.addEventListener('server:session-created', (event: Event) => {
      const customEvent = event as CustomEvent;
      this.onServerSessionCreated(customEvent.detail);
    });

    this.addEventListener('server:session-joined', (event: Event) => {
      const customEvent = event as CustomEvent;
      this.onServerSessionJoined(customEvent.detail);
    });

    this.addEventListener('server:health-updated', (event: Event) => {
      const customEvent = event as CustomEvent;
      this.onServerHealthUpdated(customEvent.detail);
    });

    this.addEventListener('server:data-updated', (event: Event) => {
      const customEvent = event as CustomEvent;
      this.onServerDataUpdated(customEvent.detail);
    });

    // Listen for data responses from server
    this.addEventListener('widget:data-received', (event: Event) => {
      const customEvent = event as CustomEvent;
      this.onDataReceived(customEvent.detail);
    });

    this.addEventListener('widget:command-complete', (event: Event) => {
      const customEvent = event as CustomEvent;
      this.onCommandComplete(customEvent.detail);
    });
  }

  /**
   * Called when a new session is created on the server
   * Override in child classes to respond to session events
   */
  protected onServerSessionCreated(event: SessionCreatedEvent): void {
    console.log(`🎛️ ${this.widgetName}: Session created [${event.sessionType}]`, {
      owner: event.owner,
      capabilities: event.capabilities
    });
  }

  /**
   * Called when a session is joined on the server
   * Override in child classes to respond to session events
   */
  protected onServerSessionJoined(event: SessionJoinedEvent): void {
    console.log(`🎛️ ${this.widgetName}: Session joined [${event.sessionType}]`, {
      joinedBy: event.joinedBy,
      userCount: event.userCount
    });
  }

  /**
   * Called when server health status is updated
   * Override in child classes to respond to health changes
   */
  protected onServerHealthUpdated(event: HealthUpdatedEvent): void {
    console.log(`🎛️ ${this.widgetName}: Health updated [${event.health.overall}]`, {
      score: event.health.score,
      changedComponents: event.changedComponents
    });
  }

  /**
   * Called when server data is updated
   * Override in child classes to respond to data changes
   */
  protected onServerDataUpdated(event: DataUpdatedEvent): void {
    console.log(`🎛️ ${this.widgetName}: Data updated [${event.dataSource}]`, {
      updateType: event.updateType,
      affectedItems: event.affectedItems.length
    });
    
    // Auto-refresh if this widget is interested in updated data
    if (this.shouldAutoRefreshOnDataUpdate(event)) {
      this.update();
    }
  }

  /**
   * Called when server data is received from fetchServerData()
   * Override in child classes to handle data responses
   */
  protected onDataReceived(response: WidgetDataResponse): void {
    if (response.success && response.data) {
      console.log(`🎛️ ${this.widgetName}: Data received [${response.dataSource}]`, {
        itemCount: response.metadata?.totalCount,
        fromCache: response.metadata?.fromCache
      });
      this.processServerData(response.dataSource, response.data);
    } else {
      console.error(`🎛️ ${this.widgetName}: Data fetch failed [${response.dataSource}]`, response.error);
      this.onDataFetchError(response.dataSource, response.error || 'Unknown error');
    }
  }

  /**
   * Called when server command completes from executeServerCommand()
   * Override in child classes to handle command responses
   */
  protected onCommandComplete(response: WidgetCommandResponse): void {
    if (response.success && response.result) {
      console.log(`🎛️ ${this.widgetName}: Command completed [${response.command}]`, {
        executionTime: response.executionTime
      });
      this.processCommandResult(response.command, response.result);
    } else {
      console.error(`🎛️ ${this.widgetName}: Command failed [${response.command}]`, response.error);
      this.onCommandError(response.command, response.error || 'Unknown error');
    }
  }

  /**
   * Override to determine if widget should auto-refresh when data changes
   * Uses strongly typed event data to make informed decisions
   */
  protected shouldAutoRefreshOnDataUpdate(_event: DataUpdatedEvent): boolean {
    // Default: no auto-refresh, widgets opt-in by overriding this method
    // Example: return event.dataSource === 'personas' && event.updateType !== 'deleted';
    return false;
  }

  /**
   * Override to process received server data
   * Type-safe processing with known data source types
   */
  protected processServerData(dataSource: DataSourceType, data: unknown): void {
    // Default: log data received - widgets should override to actually use the data
    console.log(`🎛️ ${this.widgetName}: Received ${dataSource} data - override processServerData() to use it`, data);
  }

  /**
   * Override to process command results
   * Type-safe command result processing
   */
  protected processCommandResult(command: string, result: unknown): void {
    // Default: log command result - widgets should override to actually use the result
    console.log(`🎛️ ${this.widgetName}: Command ${command} result - override processCommandResult() to use it`, result);
  }

  /**
   * Override to handle data fetch errors
   * Type-safe error handling with data source context
   */
  protected onDataFetchError(dataSource: DataSourceType, error: string): void {
    console.warn(`🎛️ ${this.widgetName}: Data fetch error [${dataSource}] - override onDataFetchError() to handle gracefully`, error);
  }

  /**
   * Override to handle command errors
   * Type-safe command error handling
   */
  protected onCommandError(command: string, error: string): void {
    console.warn(`🎛️ ${this.widgetName}: Command ${command} error - override onCommandError() to handle gracefully`, error);
  }

  /**
   * Get widget capabilities - override to declare what this widget can do
   * This enables the system to route events and validate permissions properly
   */
  protected getWidgetCapabilities(): WidgetCapabilities {
    return {
      canFetchData: [], // Override with DataSourceType[] that this widget needs
      canExecuteCommands: [], // Override with command names this widget uses  
      respondsToEvents: [], // Override with server event types this widget cares about
      supportsExport: [], // Override with export formats this widget supports
      requiresAuth: false, // Override if this widget needs authentication
      updateFrequency: 'manual' // Override with 'realtime', 'periodic', or 'manual'
    };
  }

  disconnectedCallback() {
    console.log(`🎛️ ${this.widgetName}: Disconnecting from DOM`);
    this.widgetConnected = false;
    
    // Wrap cleanup with error handler
    globalErrorHandler.safeExecute(() => {
      this.cleanup();
    }, {
      component: 'widget',
      widget: this.widgetName,
      operation: 'disconnectedCallback'
    });
  }

  /**
   * Main render method - combines CSS and HTML
   */
  async render(): Promise<void> {
    try {
      console.log(`🎨 ${this.widgetName}: Starting render() - about to loadCSS()`);
      const css = await this.loadCSS();
      console.log(`🎨 ${this.widgetName}: CSS loaded, length: ${css.length} chars`);
      
      // Auto-load HTML templates if they exist, otherwise use renderContent()
      const html = await this.loadHTMLTemplatesOrFallback();

      this.shadowRoot.innerHTML = `
        <style>
          ${css}
        </style>
        ${html}
      `;

      // Setup event listeners - DOM is ready after innerHTML assignment
      this.setupEventListeners();
      this.setupCollapseToggle();
      
    } catch (error) {
      console.error(`🎛️ ${this.widgetName}: Render failed:`, error);
      this.renderError(error);
    }
  }

  /**
   * Load CSS for the widget
   */
  async loadCSS(): Promise<string> {
    const constructor = this.constructor as typeof BaseWidget;
    
    try {
      // SMART MANIFEST: Only load CSS files that actually exist (Zero 404s!)
      const baseCSS = '/src/ui/components/shared/BaseWidget.css';
      
      // Get CSS files from build-time manifest
      const widgetAssets = window.WIDGET_ASSETS?.[constructor.name.replace(/^_/, '')];
      const cssFiles = [baseCSS];
      
      if (widgetAssets && widgetAssets.css.length > 0) {
        console.log(`📁 ${constructor.name}: Found ${widgetAssets.css.length} CSS files in manifest (Zero 404s!)`);
        const widgetCSSFiles = widgetAssets.css.map(file => `${constructor.basePath}/${file}`);
        cssFiles.push(...widgetCSSFiles);
      } else {
        console.log(`📁 ${constructor.name}: No CSS files in manifest - using BaseWidget only`);
      }
      
      console.log(`🎨 ${constructor.name}: Loading CSS files:`, cssFiles);
      const cssPromises = cssFiles.map(async (cssPath) => {
        try {
          const response = await fetch(cssPath);
          if (!response.ok) {
            // This should NEVER happen with smart manifest, but handle gracefully
            console.error(`🚨 MANIFEST ERROR: ${cssPath} not found but was in manifest!`);
            return '/* CSS failed to load - manifest error */';
          }
          const cssText = await response.text();
          console.log(`✅ Loaded CSS: ${cssPath} (${cssText.length} chars)`);
          return cssText;
        } catch (error) {
          console.error(`🚨 MANIFEST ERROR: Failed to fetch ${cssPath}:`, error);
          return '/* CSS failed to load - manifest error */';
        }
      });
      
      const cssContents = await Promise.all(cssPromises);
      const combinedCSS = cssContents.join('\n');
      
      console.log(`✅ ${constructor.name}: Loaded ${cssFiles.length} CSS files successfully (Zero 404s!)`);
      return combinedCSS;
      
    } catch (error) {
      console.warn(`Failed to load CSS for ${constructor.name}:`, error);
      return this.getDefaultBaseCSS();
    }
  }

  /**
   * Get bundled CSS - legacy method, now uses declared assets
   */
  getBundledCSS(): string | Promise<string> {
    return this.loadCSS();
  }


  /**
   * Load base CSS for collapse functionality
   */
  async loadBaseCSS(): Promise<string> {
    try {
      const response = await fetch('/src/ui/components/shared/BaseWidget.css');
      return await response.text();
    } catch (error) {
      console.warn(`🎛️ ${this.widgetName}: Failed to load base CSS, using fallback`, error);
      return this.getDefaultBaseCSS();
    }
  }

  /**
   * Fallback CSS for essential functionality if file loading fails
   */
  getDefaultBaseCSS(): string {
    return `
      /* Minimal fallback CSS for collapse functionality */
      .widget-header { cursor: pointer; padding: 12px 16px; }
      .widget-title-row { display: flex; align-items: center; gap: 8px; }
      .collapse-toggle { cursor: pointer; width: 16px; text-align: center; }
      :host(.collapsed) .widget-content { display: none; }
    `;
  }

  // Minimal methods - subclasses CAN override, but BaseWidget provides defaults
  protected renderContent(): string {
    // Try HTML templates first, fallback to renderOwnContent()
    return this.renderOwnContent();
  }
  
  protected setupEventListeners(): void {
    // Default: basic collapse functionality - subclasses can extend
    // Override this method to add your own event listeners
  }
  
  /**
   * Cleanup method for when widget is disconnected
   */
  protected cleanup(): void {
    // Override in child classes if needed
  }

  /**
   * Render error state
   */
  protected renderError(error: any): void {
    this.shadowRoot.innerHTML = `
      <style>
        .error-container {
          padding: 20px;
          background: rgba(244, 67, 54, 0.1);
          border: 1px solid rgba(244, 67, 54, 0.3);
          border-radius: 8px;
          color: #f44336;
          text-align: center;
        }
        .error-title {
          font-weight: 600;
          margin-bottom: 8px;
        }
        .error-message {
          font-size: 14px;
          opacity: 0.8;
        }
      </style>
      <div class="error-container">
        <div class="error-title">❌ ${this.widgetName} Error</div>
        <div class="error-message">${error.message || 'Unknown error occurred'}</div>
      </div>
    `;
  }

  /**
   * Update widget state and re-render
   */
  protected async update(): Promise<void> {
    if (this.widgetConnected) {
      await this.render();
    }
  }

  /**
   * Server Control Events - Like onclick but for server actions
   * Widgets can trigger server-side actions via simple event dispatch
   * Uses centralized WidgetServerControls system
   */
  
  /**
   * Take screenshot of this widget (server control event)
   */
  protected triggerScreenshot(options: any = {}): void {
    this.dispatchEvent(new CustomEvent('widget:screenshot', {
      detail: options,
      bubbles: true
    }));
  }

  /**
   * Refresh this widget from server (server control event) 
   */
  protected triggerRefresh(options: any = {}): void {
    this.dispatchEvent(new CustomEvent('widget:refresh', {
      detail: options,
      bubbles: true
    }));
  }

  /**
   * Export widget data (server control event)
   */
  protected triggerExport(format: string = 'json', options: any = {}): void {
    this.dispatchEvent(new CustomEvent('widget:export', {
      detail: { format, ...options },
      bubbles: true
    }));
  }

  /**
   * Validate widget state (server control event)
   */
  protected triggerValidate(options: any = {}): void {
    this.dispatchEvent(new CustomEvent('widget:validate', {
      detail: options,
      bubbles: true
    }));
  }

  /**
   * Fetch data from server (strongly typed server control event)
   */
  protected fetchServerData(dataSource: DataSourceType, requestOptions: Partial<WidgetDataRequest> = {}): void {
    const request: WidgetDataRequest = {
      dataSource,
      ...requestOptions // Elegant spread - merge optional params, filters, etc.
    };
    
    this.dispatchEvent(new CustomEvent('widget:fetch-data', {
      detail: request,
      bubbles: true
    }));
  }

  /**
   * Execute server command (strongly typed server control event)
   */
  protected executeServerCommand(command: string, requestOptions: Partial<WidgetCommandRequest> = {}): void {
    const request: WidgetCommandRequest = {
      command,
      ...requestOptions // Elegant spread - merge timeout, priority, params, etc.
    };
    
    this.dispatchEvent(new CustomEvent('widget:execute-command', {
      detail: request,
      bubbles: true
    }));
  }

  /**
   * Get continuum API if available
   */
  protected getContinuumAPI(): any {
    return (window as any).continuum;
  }

  /**
   * Check if continuum API is connected
   */
  protected isContinuumConnected(): boolean {
    const continuum = this.getContinuumAPI();
    return continuum && continuum.isConnected();
  }

  /**
   * Send message via continuum API (using execute with message command)
   */
  protected sendMessage(message: any): void {
    // Use execute with a generic message command instead of non-existent send method
    this.notifySystem('widget_message', message);
  }

  /**
   * Execute command via continuum API
   */
  protected async executeCommand(command: string, params: any = {}): Promise<any> {
    const continuum = this.getContinuumAPI();
    if (continuum) {
      return await continuum.execute(command, params);
    } else {
      throw new Error('Continuum API not available');
    }
  }

  /**
   * Simple widget notification system - uses WidgetDaemon queue
   */
  protected notifySystem(eventType: string, data?: any): void {
    try {
      // Try to use WidgetDaemon first (preferred approach)
      const widgetDaemon = this.getWidgetDaemon();
      if (widgetDaemon) {
        widgetDaemon.notifySystem(this.widgetName, eventType, data);
        return;
      }
      
      // Fallback: Direct logging if no daemon available
      console.log(`🔔 ${this.widgetName}: ${eventType}`, data ? { event: eventType, data } : { event: eventType });
      
      // Try to forward to continuum API as last resort
      const continuum = this.getContinuumAPI();
      if (continuum && typeof continuum.emit === 'function') {
        continuum.emit(eventType, { widget: this.widgetName, data });
      }
    } catch (error) {
      // Fail silently - notification is optional
      console.warn(`🎛️ ${this.widgetName}: Failed to notify system about ${eventType}:`, error);
    }
  }

  /**
   * Get WidgetDaemon from browser daemon system
   */
  private getWidgetDaemon(): any {
    try {
      // Access WidgetDaemon through global browser daemon controller
      const browserDaemonController = (window as any).browserDaemonController;
      if (browserDaemonController && typeof browserDaemonController.getWidgetDaemon === 'function') {
        return browserDaemonController.getWidgetDaemon();
      }
      
      // Try direct access to widgetDaemon global
      const widgetDaemon = (window as any).widgetDaemon;
      if (widgetDaemon && typeof widgetDaemon.notifySystem === 'function') {
        return widgetDaemon;
      }
      
      return null;
    } catch (error) {
      return null;
    }
  }

  /**
   * Simple command execution with graceful fallback
   */
  protected async tryExecuteCommand(command: string, params: any = {}): Promise<any> {
    try {
      const continuum = this.getContinuumAPI();
      if (continuum && typeof continuum.execute === 'function') {
        return await continuum.execute(command, params);
      } else {
        console.warn(`🎛️ ${this.widgetName}: Cannot execute command ${command} - API not available`);
        return { success: false, error: 'Continuum API not available' };
      }
    } catch (error) {
      console.warn(`🎛️ ${this.widgetName}: Command ${command} failed:`, error);
      return { success: false, error: error instanceof Error ? error.message : String(error) };
    }
  }

  /**
   * Register simple status listener - widgets can override this
   */
  protected onSystemStatus(status: string, data?: any): void {
    // Default implementation - widgets can override
    console.log(`🎛️ ${this.widgetName}: System status ${status}`, data);
  }

  /**
   * DEPRECATED: Use notifySystem() instead
   * Kept for backward compatibility with existing widgets
   */
  protected onContinuumEvent(type: string, _handler: (data: any) => void): void {
    console.warn(`⚠️ ${this.widgetName}: onContinuumEvent() is deprecated. Use notifySystem() instead.`);
    
    // Graceful fallback - just log the registration attempt
    console.log(`🔄 ${this.widgetName}: Would register event listener for ${type} (deprecated API)`);
    
    // Don't actually register anything - this prevents errors but encourages migration
  }

  /**
   * Toggle collapse state for widgets
   */
  toggleCollapse(): void {
    this.isCollapsed = !this.isCollapsed;
    console.log(`🎛️ ${this.widgetName}: ${this.isCollapsed ? 'Collapsed' : 'Expanded'}`);
    this.updateCollapseState();
  }

  /**
   * Update DOM to reflect collapse state
   */
  updateCollapseState(): void {
    const content = this.shadowRoot.querySelector('.widget-content') as HTMLElement;
    const toggle = this.shadowRoot.querySelector('.collapse-toggle') as HTMLElement;
    
    if (content) {
      if (this.isCollapsed) {
        content.style.display = 'none';
        content.style.maxHeight = '0';
        content.style.overflow = 'hidden';
      } else {
        content.style.display = '';
        content.style.maxHeight = '';
        content.style.overflow = '';
      }
    }

    if (toggle) {
      toggle.innerHTML = this.isCollapsed ? '▶' : '▼';
    }

    // Add collapsed class to host element for CSS styling
    if (this.isCollapsed) {
      this.classList.add('collapsed');
    } else {
      this.classList.remove('collapsed');
    }
  }

  /**
   * Render error state when widget initialization fails
   */
  protected async renderErrorState(error: any): Promise<void> {
    if (!this.shadowRoot) return;
    
    const errorMessage = error instanceof Error ? error.message : String(error);
    
    // Don't call console.error here to avoid infinite loops when server is down
    // The error is already captured by the calling globalErrorHandler.safeExecute()

    this.shadowRoot.innerHTML = `
      <style>
        .error-widget {
          background: #ffe6e6;
          border: 2px solid #ff9999;
          border-radius: 8px;
          padding: 16px;
          margin: 8px;
          font-family: monospace;
          color: #cc0000;
        }
        .error-title {
          font-weight: bold;
          margin-bottom: 8px;
        }
        .error-message {
          margin-bottom: 8px;
          font-size: 14px;
        }
        .error-time {
          font-size: 12px;
          color: #666;
        }
      </style>
      <div class="error-widget">
        <div class="error-title">⚠️ ${this.widgetName} Error</div>
        <div class="error-message">${errorMessage}</div>
        <div class="error-time">${new Date().toLocaleTimeString()}</div>
      </div>
    `;
  }

  /**
   * Setup collapse toggle functionality
   */
  setupCollapseToggle(): void {
    const toggle = this.shadowRoot.querySelector('.collapse-toggle') as HTMLElement;
    if (toggle) {
      toggle.addEventListener('click', (e: Event) => {
        e.preventDefault();
        e.stopPropagation();
        this.toggleCollapse();
      });
    }
  }

  /**
   * Render widget with collapsible header
   */
  renderWithCollapseHeader(content: string): string {
    return `
      <div class="widget-header">
        <div class="widget-title-row">
          <span class="collapse-toggle">▼</span>
          <span class="widget-icon">${this.widgetIcon}</span>
          <span class="widget-title">${this.widgetTitle}</span>
        </div>
      </div>
      <div class="widget-content">
        ${content}
      </div>
    `;
  }
}