/**
 * BaseWidget - Strongly Typed Abstract Base Class for All Widgets
 * Enforces proper implementation through abstract methods and properties
 */

// Import smart asset manifest (zero 404s!)
import { WIDGET_ASSETS } from 'widget-discovery';


export abstract class BaseWidget extends HTMLElement {
  declare shadowRoot: ShadowRoot;
  
  // Properties that subclasses can set in constructor
  protected widgetName: string = 'BaseWidget';
  protected widgetIcon: string = '🔹';
  protected widgetTitle: string = 'Widget';
  
  // Protected state
  protected widgetConnected: boolean = false;
  protected isCollapsed: boolean = false;

  // Smart defaults - minimal requirements from subclasses
  public static getBasePath(): string {
    // Auto-derive from class name: ChatWidget -> /src/ui/components/Chat
    // Handle TypeScript compilation artifacts like _PersonaWidget -> PersonaWidget
    const rawName = this.name.replace(/^_/, ''); // Remove leading underscore
    const className = rawName.replace('Widget', '');
    return `/src/ui/components/${className}`;
  }
  
  protected static getOwnCSS(): ReadonlyArray<string> {
    // DISCOVERY-FIRST: Auto-discover ALL CSS files in widget directory
    // Can be overridden for performance or specific control
    return this.discoverAllCSS();
  }
  
  protected static getOwnHTML(): ReadonlyArray<string> {
    // DISCOVERY-FIRST: Auto-discover ALL HTML files in widget directory  
    // Can be overridden for specific template control
    return this.discoverAllHTML();
  }
  
  /**
   * Discovery method: Find all CSS files in widget directory
   * Override this method for custom discovery logic
   */
  protected static discoverAllCSS(): ReadonlyArray<string> {
    // Use package.json files array for discovery
    return this.getAssetsByExtension('.css');
  }
  
  /**
   * Discovery method: Find all HTML files in widget directory
   * Override this method for custom discovery logic
   */
  protected static discoverAllHTML(): ReadonlyArray<string> {
    // Use package.json files array for discovery
    return this.getAssetsByExtension('.html');
  }
  
  /**
   * Get assets by file extension from package.json files array
   * Falls back to convention if package.json not available
   */
  private static getAssetsByExtension(extension: string): ReadonlyArray<string> {
    // For now, use the package.json discovery we already have
    // This is synchronous fallback - async discovery happens in loadCSS()
    const cleanName = this.name.replace(/^_/, ''); // Remove TypeScript compilation underscore
    if (extension === '.css') {
      return [`${cleanName}.css`]; // Convention fallback
    } else if (extension === '.html') {
      return [`${cleanName}.html`]; // Convention fallback
    }
    return [];
  }
  
  /**
   * Get widget files from package.json - replaces manual CSS/HTML declarations
   * Reads the 'files' array from widget's package.json automatically
   */
  static async getWidgetFiles(): Promise<string[]> {
    try {
      const basePath = this.getBasePath().replace('/dist/', '/src/'); // Read from source
      const packagePath = `${basePath}/package.json`;
      
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
    return assets.map(file => `${this.getBasePath()}/${file}`);
  }

  
  /**
   * Auto-load HTML templates or fallback to renderContent() - zero burden
   */
  protected async loadHTMLTemplatesOrFallback(): Promise<string> {
    try {
      const constructor = this.constructor as typeof BaseWidget;
      const widgetName = constructor.name.replace(/^_/, ''); // Handle TypeScript compilation artifacts
      const basePath = constructor.getBasePath();
      
      // SMART MANIFEST: Only load HTML files that actually exist (Zero 404s!)
      const widgetAssets = WIDGET_ASSETS[widgetName];
      
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
        fetch(`${constructor.getBasePath()}/${file}`).then(r => r.text())
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
    console.log(`🏗️ ${this.constructor.name}: Constructor called`);
    this.attachShadow({ mode: 'open' });
    console.log(`🏗️ ${this.constructor.name}: Shadow DOM attached`);
  }

  async connectedCallback() {
    console.log(`🎛️ ${this.widgetName}: connectedCallback() triggered - connecting to DOM`);
    this.widgetConnected = true;
    console.log(`🎛️ ${this.widgetName}: About to call initializeWidget()`);
    await this.initializeWidget();
    console.log(`🎛️ ${this.widgetName}: About to call render()`);
    await this.render();
    console.log(`🎛️ ${this.widgetName}: connectedCallback() complete`);
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
   * Load CSS for the widget - uses declarative getOwnCSS() method only
   */
  async loadCSS(): Promise<string> {
    const constructor = this.constructor as typeof BaseWidget;
    
    try {
      // SMART MANIFEST: Only load CSS files that actually exist (Zero 404s!)
      const widgetName = constructor.name.replace(/^_/, ''); // Handle TypeScript compilation artifacts
      const basePath = constructor.getBasePath();
      const baseCSS = '/src/ui/components/shared/BaseWidget.css';
      
      // Get CSS files from build-time manifest
      const widgetAssets = WIDGET_ASSETS[widgetName];
      const cssFiles = [baseCSS];
      
      if (widgetAssets && widgetAssets.css.length > 0) {
        console.log(`📁 ${constructor.name}: Found ${widgetAssets.css.length} CSS files in manifest (Zero 404s!)`);
        const widgetCSSFiles = widgetAssets.css.map(file => `${basePath}/${file}`);
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