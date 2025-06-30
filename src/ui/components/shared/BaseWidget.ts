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
    // CSS is now bundled with the widget - return bundled styles
    return this.getBundledCSS();
  }

  /**
   * Get bundled CSS - override in child classes to provide their CSS
   */
  getBundledCSS(): string {
    return this.getDefaultBaseCSS();
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