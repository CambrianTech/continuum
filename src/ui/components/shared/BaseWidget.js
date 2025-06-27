/**
 * BaseWidget - TypeScript Base Class for All Widgets
 * Provides common functionality and clean separation of concerns
 * Standardized CSS loading, event handling, and lifecycle management
 */
export class BaseWidget extends HTMLElement {
    constructor() {
        super();
        this.widgetName = 'BaseWidget';
        this.widgetIcon = '🔹';
        this.widgetTitle = 'Widget';
        this.widgetConnected = false;
        this.cssPath = ''; // Override in child classes
        this.cachedCSS = null;
        this.isCollapsed = false; // Track collapse state
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
    async initializeWidget() {
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
    async render() {
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
        }
        catch (error) {
            console.error(`🎛️ ${this.widgetName}: Render failed:`, error);
            this.renderError(error);
        }
    }
    /**
     * Load CSS for the widget with caching and error handling
     */
    async loadCSS() {
        if (this.cachedCSS) {
            return this.cachedCSS;
        }
        if (!this.cssPath) {
            console.warn(`🎛️ ${this.widgetName}: No CSS path specified`);
            return '';
        }
        try {
            const response = await fetch(this.cssPath);
            if (!response.ok) {
                throw new Error(`HTTP ${response.status}: ${response.statusText}`);
            }
            this.cachedCSS = await response.text();
            return this.cachedCSS;
        }
        catch (error) {
            console.error(`🎛️ ${this.widgetName}: Failed to load CSS from ${this.cssPath}:`, error);
            return `/* CSS loading failed: ${error.message} */`;
        }
    }
    /**
     * Cleanup method for when widget is disconnected
     */
    cleanup() {
        // Override in child classes if needed
    }
    /**
     * Render error state
     */
    renderError(error) {
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
    async update() {
        if (this.widgetConnected) {
            await this.render();
        }
    }
    /**
     * Get continuum API if available
     */
    getContinuumAPI() {
        return window.continuum;
    }
    /**
     * Check if continuum API is connected
     */
    isContinuumConnected() {
        const continuum = this.getContinuumAPI();
        return continuum && continuum.isConnected();
    }
    /**
     * Send message via continuum API
     */
    sendMessage(message) {
        const continuum = this.getContinuumAPI();
        if (continuum) {
            continuum.send(message);
        }
        else {
            console.warn(`🎛️ ${this.widgetName}: Continuum API not available`);
        }
    }
    /**
     * Execute command via continuum API
     */
    async executeCommand(command, params = {}) {
        const continuum = this.getContinuumAPI();
        if (continuum) {
            return await continuum.execute(command, params);
        }
        else {
            throw new Error('Continuum API not available');
        }
    }
    /**
     * Register event listener with continuum API
     */
    onContinuumEvent(type, handler) {
        const continuum = this.getContinuumAPI();
        if (continuum) {
            continuum.on(type, handler);
        }
        else {
            console.warn(`🎛️ ${this.widgetName}: Cannot register event ${type} - Continuum API not available`);
        }
    }
    /**
     * Remove event listener from continuum API
     */
    offContinuumEvent(type, handler) {
        const continuum = this.getContinuumAPI();
        if (continuum) {
            continuum.off(type, handler);
        }
    }

    /**
     * Toggle collapse state for widgets
     */
    toggleCollapse() {
        this.isCollapsed = !this.isCollapsed;
        console.log(`🎛️ ${this.widgetName}: ${this.isCollapsed ? 'Collapsed' : 'Expanded'}`);
        this.updateCollapseState();
    }

    /**
     * Update DOM to reflect collapse state
     */
    updateCollapseState() {
        const content = this.shadowRoot.querySelector('.widget-content');
        const toggle = this.shadowRoot.querySelector('.collapse-toggle');
        
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
            toggle.style.transform = this.isCollapsed ? 'rotate(0deg)' : 'rotate(0deg)';
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
    setupCollapseToggle() {
        const toggle = this.shadowRoot.querySelector('.collapse-toggle');
        if (toggle) {
            toggle.addEventListener('click', (e) => {
                e.preventDefault();
                e.stopPropagation();
                this.toggleCollapse();
            });
        }
    }

    /**
     * Render widget with collapsible header
     */
    renderWithCollapseHeader(content) {
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