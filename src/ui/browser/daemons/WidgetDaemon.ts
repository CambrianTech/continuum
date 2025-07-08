/**
 * Browser Widget Daemon - Extracted from continuum-browser.ts
 * 
 * Handles widget discovery, loading, health validation, and event management
 * Part of the modular browser daemon architecture
 */

export interface WidgetHealthComponent {
  component: string;
  status: 'healthy' | 'degraded' | 'failed';
  lastCheck: number;
  details: string;
  metrics: {
    hasElement: boolean;
    isVisible: boolean;
    hasStyles: boolean;
    [key: string]: any;
  };
}

export interface WidgetDiscoveryResult {
  totalWidgets: number;
  loadedWidgets: string[];
  customElements: Element[];
  discoveryMethod: 'html-dom' | 'dynamic-injection' | 'renderer-daemon';
}

export class WidgetDaemon {
  private eventHandlers = new Map<string, ((data: any) => void)[]>();
  private knownWidgets = ['chat-widget', 'continuum-sidebar'];
  private widgetLoadingComplete = false;

  constructor() {
    console.log('🎨 WidgetDaemon: Initializing widget management system');
  }

  /**
   * Initialize the widget daemon
   */
  async initialize(): Promise<void> {
    console.log('🎨 WidgetDaemon: Starting widget system initialization');
    
    // Set up event listeners for widget lifecycle
    this.setupWidgetEventListeners();
    
    console.log('✅ WidgetDaemon: Initialized successfully');
  }

  /**
   * Discover and load widgets in the DOM
   */
  async discoverAndLoadWidgets(): Promise<WidgetDiscoveryResult> {
    console.log('🔍 WidgetDaemon: Starting widget discovery...');
    
    // ARCHITECTURAL DECISION: RendererDaemon handles all widget discovery/injection
    // Browser client just initializes what's already in the HTML
    console.log('📋 RendererDaemon: Discovers widgets, bundles assets, injects into HTML');
    console.log('🏗️ Browser: Initializes custom elements that are already in DOM');
    
    // Discover custom elements in the DOM
    const customElements = document.querySelectorAll('continuum-sidebar, chat-widget');
    console.log(`✅ Found ${customElements.length} custom elements in DOM`);
    
    // Extract widget names from elements
    const loadedWidgets = Array.from(customElements).map(element => element.tagName.toLowerCase());
    
    // Simplified: Just count existing elements (no async loading needed)
    const totalWidgets = customElements.length;
    console.log(`✅ Widget loading complete - ${totalWidgets} widgets found in DOM`);
    
    // Widgets are already instantiated in the HTML via RendererDaemon
    console.log('🎨 Widgets ready (instantiated via HTML)');
    
    this.widgetLoadingComplete = true;
    
    // Emit widget discovery complete event
    this.emit('widgets:discovery_complete', {
      totalWidgets,
      loadedWidgets,
      customElements: Array.from(customElements)
    });
    
    return {
      totalWidgets,
      loadedWidgets,
      customElements: Array.from(customElements),
      discoveryMethod: 'html-dom'
    };
  }

  /**
   * Validate widget health and return detailed component status
   */
  async validateWidgetHealth(): Promise<WidgetHealthComponent[]> {
    console.log('🏥 WidgetDaemon: Starting widget health validation');
    
    const healthComponents: WidgetHealthComponent[] = [];
    
    // Check each known widget
    for (const widgetName of this.knownWidgets) {
      try {
        const widget = document.querySelector(widgetName);
        const widgetStyles = widget ? window.getComputedStyle(widget) : null;
      
      const healthComponent: WidgetHealthComponent = {
        component: widgetName,
        status: widget ? 'healthy' : 'failed',
        lastCheck: Date.now(),
        details: widget ? 'Widget element present and styled' : 'Widget element missing from DOM',
        metrics: {
          hasElement: !!widget,
          isVisible: widgetStyles ? widgetStyles.display !== 'none' : false,
          hasStyles: widgetStyles ? widgetStyles.cssText.length > 0 : false,
          computedStylesCount: widgetStyles ? Object.keys(widgetStyles).length : 0
        }
      };
      
      // Additional checks for degraded status
      if (widget && widgetStyles) {
        if (widgetStyles.display === 'none' || widgetStyles.visibility === 'hidden') {
          healthComponent.status = 'degraded';
          healthComponent.details = 'Widget element present but not visible';
        } else if (widgetStyles.cssText.length === 0) {
          healthComponent.status = 'degraded';
          healthComponent.details = 'Widget element present but no styles applied';
        }
      }
      
        healthComponents.push(healthComponent);
      } catch (error) {
        // Handle DOM query errors gracefully
        console.warn(`🏥 WidgetDaemon: Error checking widget ${widgetName}:`, error);
        
        // Create failed health component for widget that couldn't be checked
        healthComponents.push({
          component: widgetName,
          status: 'failed',
          lastCheck: Date.now(),
          details: `Widget health check failed: ${error instanceof Error ? error.message : String(error)}`,
          metrics: {
            hasElement: false,
            isVisible: false,
            hasStyles: false,
            error: error instanceof Error ? error.message : String(error)
          }
        });
      }
    }
    
    console.log(`🏥 WidgetDaemon: Health check complete - ${healthComponents.length} widgets checked`);
    
    return healthComponents;
  }

  /**
   * Set up widget-specific event listeners
   */
  private setupWidgetEventListeners(): void {
    // Listen for DOM changes that might affect widgets
    if (typeof MutationObserver !== 'undefined') {
      const observer = new MutationObserver((mutations) => {
        let widgetChanges = false;
        
        mutations.forEach((mutation) => {
          if (mutation.type === 'childList') {
            mutation.addedNodes.forEach((node) => {
              if (node.nodeType === Node.ELEMENT_NODE) {
                const element = node as Element;
                if (this.knownWidgets.some(widget => element.tagName.toLowerCase() === widget)) {
                  widgetChanges = true;
                }
              }
            });
          }
        });
        
        if (widgetChanges) {
          console.log('🎨 WidgetDaemon: Widget changes detected in DOM');
          this.emit('widgets:dom_changed', { timestamp: Date.now() });
        }
      });
      
      observer.observe(document.body, {
        childList: true,
        subtree: true
      });
    }
  }

  /**
   * Handle widget-specific ready events
   */
  async handleContinuumReady(): Promise<void> {
    console.log('🎨 WidgetDaemon: Handling continuum:ready event');
    
    // Ensure widgets are discovered if not already done
    if (!this.widgetLoadingComplete) {
      await this.discoverAndLoadWidgets();
    }
    
    // Emit widget-specific ready event
    this.emit('widgets:ready', {
      timestamp: Date.now(),
      widgetCount: this.knownWidgets.length
    });
  }

  /**
   * Get widget-specific status information
   */
  async getWidgetStatus(): Promise<any> {
    const healthComponents = await this.validateWidgetHealth();
    const discoveryResult = await this.discoverAndLoadWidgets();
    
    return {
      widgetLoadingComplete: this.widgetLoadingComplete,
      totalWidgets: discoveryResult.totalWidgets,
      loadedWidgets: discoveryResult.loadedWidgets,
      healthComponents,
      knownWidgets: this.knownWidgets,
      status: {
        healthy: healthComponents.filter(c => c.status === 'healthy').length,
        degraded: healthComponents.filter(c => c.status === 'degraded').length,
        failed: healthComponents.filter(c => c.status === 'failed').length
      }
    };
  }

  /**
   * Add event listener
   */
  on(event: string, handler: (data: any) => void): void {
    if (!this.eventHandlers.has(event)) {
      this.eventHandlers.set(event, []);
    }
    this.eventHandlers.get(event)!.push(handler);
  }

  /**
   * Remove event listener
   */
  off(event: string, handler?: (data: any) => void): void {
    const handlers = this.eventHandlers.get(event);
    if (!handlers) return;

    if (handler) {
      const index = handlers.indexOf(handler);
      if (index > -1) {
        handlers.splice(index, 1);
      }
    } else {
      // Remove all handlers for this event
      this.eventHandlers.set(event, []);
    }
  }

  /**
   * Emit event to all listeners
   */
  emit(event: string, data?: any): void {
    const handlers = this.eventHandlers.get(event);
    if (handlers) {
      handlers.forEach(handler => {
        try {
          handler(data);
        } catch (error) {
          console.error(`🎨 WidgetDaemon: Error in event handler for ${event}:`, error);
        }
      });
    }
  }

  /**
   * Register a new widget type for monitoring
   */
  registerWidget(widgetName: string): void {
    if (!this.knownWidgets.includes(widgetName)) {
      this.knownWidgets.push(widgetName);
      console.log(`🎨 WidgetDaemon: Registered new widget type: ${widgetName}`);
    }
  }

  /**
   * Unregister a widget type
   */
  unregisterWidget(widgetName: string): void {
    const index = this.knownWidgets.indexOf(widgetName);
    if (index > -1) {
      this.knownWidgets.splice(index, 1);
      console.log(`🎨 WidgetDaemon: Unregistered widget type: ${widgetName}`);
    }
  }

  /**
   * Get list of known widget types
   */
  getKnownWidgets(): string[] {
    return [...this.knownWidgets];
  }

  /**
   * Check if widget loading is complete
   */
  isWidgetLoadingComplete(): boolean {
    return this.widgetLoadingComplete;
  }

  /**
   * Clean up resources
   */
  destroy(): void {
    // Clear event handlers
    this.eventHandlers.clear();
    
    // Reset state
    this.widgetLoadingComplete = false;
    
    console.log('🎨 WidgetDaemon: Destroyed and cleaned up');
  }
}

// Export singleton instance
export const widgetDaemon = new WidgetDaemon();