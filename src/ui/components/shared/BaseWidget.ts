/**
 * BaseWidget - TypeScript Base Class for All Widgets
 * Provides common functionality and clean separation of concerns
 * Standardized CSS loading, event handling, and lifecycle management
 */

export abstract class BaseWidget extends HTMLElement {
  declare shadowRoot: ShadowRoot;
  protected widgetName: string = 'BaseWidget';
  protected widgetIcon: string = '🔹';
  protected widgetTitle: string = 'Widget';
  protected widgetConnected: boolean = false;
  protected cssPath: string = ''; // Override in child classes
  protected isCollapsed: boolean = false; // Track collapse state

  /**
   * Widget reports its own base path for asset resolution
   * Override in child classes to specify the widget's directory
   */
  static getBasePath(): string {
    return '/src/ui/components/shared';
  }
  
  /**
   * Widget declares its own CSS files (BaseWidget CSS included automatically)
   * Override in child classes to specify additional CSS files needed
   */
  static getOwnCSS(): string[] {
    return []; // Child widgets override this
  }
  
  /**
   * Widget declares its own HTML files (optional)
   * Override in child classes to specify HTML template files
   */
  static getOwnHTML(): string[] {
    return []; // Child widgets can override this
  }
  
  /**
   * Get all CSS assets including base widget CSS
   */
  static getCSSAssets(): string[] {
    const baseCSS = ['/src/ui/components/shared/BaseWidget.css'];
    const ownCSS = this.getOwnCSS().map(css => `${this.getBasePath()}/${css}`);
    return [...baseCSS, ...ownCSS];
  }
  
  /**
   * Load HTML templates if widget declares any
   */
  protected async loadHTMLTemplates(): Promise<string> {
    const constructor = this.constructor as typeof BaseWidget;
    const basePath = constructor.getBasePath();
    const htmlFiles = constructor.getOwnHTML();
    
    if (htmlFiles.length === 0) {
      return this.renderOwnContent(); // Fallback to code-based content
    }
    
    try {
      const htmlPromises = htmlFiles.map(file => 
        fetch(`${basePath}/${file}`).then(r => r.text())
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
    const basePath = (this.constructor as typeof BaseWidget).getBasePath();
    return `${basePath}/${relativePath}`;
  }

  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
  }

  async connectedCallback() {
    console.log(`🎛️ ${this.widgetName}: Connecting to DOM`);
    this.widgetConnected = true;
    await this.initializeWidget();
    await this.render();
  }

  /**
   * Initialize widget - override for custom initialization
   */
  protected async initializeWidget(): Promise<void> {
    // Override in child classes for custom initialization
  }

  disconnectedCallback() {
    console.log(`🎛️ ${this.widgetName}: Disconnecting from DOM`);
    this.widgetConnected = false;
    this.cleanup();
  }

  /**
   * Main render method - combines CSS and HTML
   */
  async render(): Promise<void> {
    try {
      const css = await this.loadCSS();
      const html = this.renderContent();

      this.shadowRoot.innerHTML = `
        <style>
          ${css}
        </style>
        ${html}
      `;

      // Setup event listeners after DOM is ready
      setTimeout(() => {
        this.setupEventListeners();
        this.setupCollapseToggle();
      }, 0);
      
    } catch (error) {
      console.error(`🎛️ ${this.widgetName}: Render failed:`, error);
      this.renderError(error);
    }
  }

  /**
   * Load CSS for the widget - now uses bundled CSS
   */
  async loadCSS(): Promise<string> {
    // Load CSS based on declared assets with proper error handling
    const constructor = this.constructor as typeof BaseWidget;
    const cssAssets = constructor.getCSSAssets();
    
    try {
      const cssPromises = cssAssets.map(async (assetPath) => {
        try {
          const response = await fetch(assetPath);
          if (!response.ok) {
            console.warn(`Failed to load CSS asset ${assetPath}: HTTP ${response.status}`);
            return '/* CSS asset failed to load */';
          }
          return await response.text();
        } catch (error) {
          console.warn(`Failed to fetch CSS asset ${assetPath}:`, error);
          return '/* CSS asset failed to load */';
        }
      });
      
      const cssContents = await Promise.all(cssPromises);
      const combinedCSS = cssContents.join('\n');
      
      console.log(`✅ ${constructor.name}: Loaded ${cssAssets.length} CSS assets`);
      return combinedCSS;
      
    } catch (error) {
      console.warn(`Failed to load CSS assets for ${constructor.name}:`, error);
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

  /**
   * Render the HTML content - must be implemented by child classes
   */
  abstract renderContent(): string;

  /**
   * Setup event listeners - must be implemented by child classes
   */
  abstract setupEventListeners(): void;

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
   * Send message via continuum API
   */
  protected sendMessage(message: any): void {
    const continuum = this.getContinuumAPI();
    if (continuum) {
      continuum.send(message);
    } else {
      console.warn(`🎛️ ${this.widgetName}: Continuum API not available`);
    }
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
   * Register event listener with continuum API
   */
  protected onContinuumEvent(type: string, handler: (data: any) => void): void {
    const continuum = this.getContinuumAPI();
    if (continuum) {
      continuum.on(type, handler);
    } else {
      console.warn(`🎛️ ${this.widgetName}: Cannot register event ${type} - Continuum API not available`);
    }
  }

  /**
   * Remove event listener from continuum API
   */
  protected offContinuumEvent(type: string, handler?: (data: any) => void): void {
    const continuum = this.getContinuumAPI();
    if (continuum) {
      continuum.off(type, handler);
    }
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