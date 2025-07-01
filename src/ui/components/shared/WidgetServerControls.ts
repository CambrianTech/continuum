/**
 * Widget Server Controls - Event-driven integration points
 * Allows widgets to trigger server-side actions via custom events
 * Classic server controls pattern for modern web components
 */

export interface WidgetServerEvent {
  widgetId: string;
  eventType: string;
  data?: any;
  timestamp: Date;
}

export class WidgetServerControls {
  private static instance: WidgetServerControls;
  
  static getInstance(): WidgetServerControls {
    if (!WidgetServerControls.instance) {
      WidgetServerControls.instance = new WidgetServerControls();
    }
    return WidgetServerControls.instance;
  }

  private constructor() {
    this.setupEventListeners();
  }

  /**
   * Setup global event listeners for widget server control events
   */
  private setupEventListeners(): void {
    // Listen for custom widget events that should trigger server actions
    document.addEventListener('widget:screenshot', this.handleScreenshotEvent.bind(this));
    document.addEventListener('widget:refresh', this.handleRefreshEvent.bind(this));
    document.addEventListener('widget:export', this.handleExportEvent.bind(this));
    document.addEventListener('widget:validate', this.handleValidateEvent.bind(this));
  }

  /**
   * Handle widget screenshot events
   */
  private async handleScreenshotEvent(event: CustomEvent): Promise<void> {
    try {
      console.log('📸 Server Control: Widget screenshot requested', event.detail);
      
      const widgetElement = event.target as HTMLElement;
      const widgetId = widgetElement.tagName.toLowerCase();
      
      // Route to screenshot command via Continuum API
      const continuum = (window as any).continuum;
      if (continuum) {
        const result = await continuum.execute('screenshot', {
          target: 'widget',
          widgetId: widgetId,
          selector: widgetElement.tagName.toLowerCase(),
          includeContext: true,
          ...event.detail
        });
        
        // Notify widget of completion via custom event
        widgetElement.dispatchEvent(new CustomEvent('widget:screenshot-complete', {
          detail: { success: true, result }
        }));
        
        console.log('✅ Widget screenshot completed:', result);
      } else {
        throw new Error('Continuum API not available');
      }
      
    } catch (error) {
      console.error('❌ Widget screenshot failed:', error);
      
      // Notify widget of failure
      const widgetElement = event.target as HTMLElement;
      widgetElement.dispatchEvent(new CustomEvent('widget:screenshot-complete', {
        detail: { 
          success: false, 
          error: error instanceof Error ? error.message : String(error)
        }
      }));
    }
  }

  /**
   * Handle widget refresh events
   */
  private async handleRefreshEvent(event: CustomEvent): Promise<void> {
    try {
      console.log('🔄 Server Control: Widget refresh requested', event.detail);
      
      const widgetElement = event.target as HTMLElement;
      const widgetId = widgetElement.tagName.toLowerCase();
      
      const continuum = (window as any).continuum;
      if (continuum) {
        const result = await continuum.execute('reload', {
          target: 'widget',
          widgetId: widgetId,
          preserveState: event.detail?.preserveState || true,
          ...event.detail
        });
        
        widgetElement.dispatchEvent(new CustomEvent('widget:refresh-complete', {
          detail: { success: true, result }
        }));
        
        console.log('✅ Widget refresh completed:', result);
      }
      
    } catch (error) {
      console.error('❌ Widget refresh failed:', error);
      const widgetElement = event.target as HTMLElement;
      widgetElement.dispatchEvent(new CustomEvent('widget:refresh-complete', {
        detail: { success: false, error: String(error) }
      }));
    }
  }

  /**
   * Handle widget export events
   */
  private async handleExportEvent(event: CustomEvent): Promise<void> {
    try {
      console.log('💾 Server Control: Widget export requested', event.detail);
      
      const widgetElement = event.target as HTMLElement;
      const widgetId = widgetElement.tagName.toLowerCase();
      
      const continuum = (window as any).continuum;
      if (continuum) {
        const result = await continuum.execute('export', {
          target: 'widget',
          widgetId: widgetId,
          format: event.detail?.format || 'json',
          ...event.detail
        });
        
        widgetElement.dispatchEvent(new CustomEvent('widget:export-complete', {
          detail: { success: true, result }
        }));
        
        console.log('✅ Widget export completed:', result);
      }
      
    } catch (error) {
      console.error('❌ Widget export failed:', error);
      const widgetElement = event.target as HTMLElement;
      widgetElement.dispatchEvent(new CustomEvent('widget:export-complete', {
        detail: { success: false, error: String(error) }
      }));
    }
  }

  /**
   * Handle widget validation events
   */
  private async handleValidateEvent(event: CustomEvent): Promise<void> {
    try {
      console.log('✅ Server Control: Widget validation requested', event.detail);
      
      const widgetElement = event.target as HTMLElement;
      const widgetId = widgetElement.tagName.toLowerCase();
      
      const continuum = (window as any).continuum;
      if (continuum) {
        const result = await continuum.execute('validate', {
          target: 'widget',
          widgetId: widgetId,
          validateAssets: true,
          validateContent: true,
          ...event.detail
        });
        
        widgetElement.dispatchEvent(new CustomEvent('widget:validate-complete', {
          detail: { success: true, result }
        }));
        
        console.log('✅ Widget validation completed:', result);
      }
      
    } catch (error) {
      console.error('❌ Widget validation failed:', error);
      const widgetElement = event.target as HTMLElement;
      widgetElement.dispatchEvent(new CustomEvent('widget:validate-complete', {
        detail: { success: false, error: String(error) }
      }));
    }
  }
}

// Initialize server controls when module loads
WidgetServerControls.getInstance();