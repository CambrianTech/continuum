/**
 * JTAG Event System - Promise-based Triggers and Event Listeners
 * 
 * Provides reactive programming model for JTAG debugging with:
 * - Promise-based triggers that resolve when conditions are met
 * - Event listeners for real-time monitoring
 * - Automatic retry mechanisms with exponential backoff
 * - Cross-platform event correlation
 */

import { EventEmitter } from 'events';
import {
  JTAGProbeMethod,
  WidgetAnalysisData,
  HealthAnalysisData
} from '../../../shared/types/JTAGSharedTypes';

export interface JTAGTriggerCondition {
  name: string;
  description: string;
  probe: JTAGProbeMethod;
  condition: (data: any) => boolean;
  timeout?: number;
  retryInterval?: number;
  maxRetries?: number;
}

export interface JTAGEventHandler {
  event: string;
  handler: (data: any) => void | Promise<void>;
  once?: boolean;
}

export interface JTAGWaitOptions {
  timeout?: number;
  retryInterval?: number;
  maxRetries?: number;
  onRetry?: (attempt: number, data: any) => void;
  onProgress?: (data: any) => void;
}

export class JTAGEventSystem extends EventEmitter {
  private activeWaiters: Map<string, Promise<any>> = new Map();
  private retryTimers: Map<string, NodeJS.Timeout> = new Map();
  private browserEventListeners: Map<string, (data: any) => void> = new Map();

  constructor() {
    super();
    this.setupBrowserEventBridge();
  }

  /**
   * Wait for a condition to be met with promise-based resolution
   */
  public async waitFor<T = any>(
    condition: JTAGTriggerCondition,
    options: JTAGWaitOptions = {}
  ): Promise<T> {
    const {
      timeout = 30000,
      retryInterval = 1000,
      maxRetries = 30,
      onRetry,
      onProgress
    } = options;

    const waiterId = `${condition.name}-${Date.now()}`;
    
    if (this.activeWaiters.has(waiterId)) {
      return this.activeWaiters.get(waiterId)!;
    }

    const promise = new Promise<T>((resolve, reject) => {
      let retryCount = 0;
      
      const checkCondition = async () => {
        try {
          // Execute probe to get current data
          const probeData = await this.executeProbe(condition.probe);
          
          // Call progress callback
          if (onProgress) {
            onProgress(probeData);
          }

          // Check if condition is met
          if (condition.condition(probeData)) {
            this.cleanup(waiterId);
            resolve(probeData);
            return;
          }

          // Check retry limits
          if (retryCount >= maxRetries) {
            this.cleanup(waiterId);
            reject(new Error(`Condition '${condition.name}' not met after ${maxRetries} retries`));
            return;
          }

          // Call retry callback
          if (onRetry) {
            onRetry(retryCount + 1, probeData);
          }

          retryCount++;
          
          // Schedule next retry
          const timer = setTimeout(checkCondition, retryInterval);
          this.retryTimers.set(waiterId, timer);

        } catch (error) {
          this.cleanup(waiterId);
          reject(error);
        }
      };

      // Set overall timeout
      const timeoutTimer = setTimeout(() => {
        this.cleanup(waiterId);
        reject(new Error(`Condition '${condition.name}' timed out after ${timeout}ms`));
      }, timeout);

      // Store timeout for cleanup
      this.retryTimers.set(`${waiterId}-timeout`, timeoutTimer);

      // Start checking
      checkCondition();
    });

    this.activeWaiters.set(waiterId, promise);
    return promise;
  }

  /**
   * Wait for widgets to be rendered
   */
  public async waitForWidgetsRendered(widgetNames?: string[], options?: JTAGWaitOptions): Promise<WidgetAnalysisData> {
    return this.waitFor({
      name: 'widgets-rendered',
      description: `Wait for widgets ${widgetNames ? widgetNames.join(', ') : 'all'} to be rendered`,
      probe: 'widgets',
      condition: (data: WidgetAnalysisData) => {
        if (widgetNames && widgetNames.length > 0) {
          // Check specific widgets
          return widgetNames.every(name => 
            data.widgets.some(w => w.name === name && w.isRendered)
          );
        } else {
          // Check all widgets are rendered
          return data.widgets.length > 0 && data.widgets.every(w => w.isRendered);
        }
      }
    }, options);
  }

  /**
   * Wait for system to be healthy
   */
  public async waitForHealthy(minScore = 80, options?: JTAGWaitOptions): Promise<HealthAnalysisData> {
    return this.waitFor({
      name: 'system-healthy',
      description: `Wait for system health score >= ${minScore}`,
      probe: 'health',
      condition: (data: HealthAnalysisData) => {
        return data.overall === 'healthy' && data.score >= minScore;
      }
    }, options);
  }

  /**
   * Wait for custom element registration
   */
  public async waitForCustomElements(elementNames: string[], options?: JTAGWaitOptions) {
    return this.waitFor({
      name: 'custom-elements-registered',
      description: `Wait for custom elements ${elementNames.join(', ')} to be registered`,
      probe: 'customElements',
      condition: (data) => {
        return elementNames.every(name =>
          data.registry.some((reg: any) => reg.name === name && reg.defined)
        );
      }
    }, options);
  }

  /**
   * Wait for shadow DOM content to appear
   */
  public async waitForShadowContent(selector: string, minLength = 10, options?: JTAGWaitOptions) {
    return this.waitFor({
      name: 'shadow-content-ready',
      description: `Wait for shadow DOM content in ${selector}`,
      probe: 'shadowDOM',
      condition: (data) => {
        return data.elements.some((el: any) => 
          el.selector === selector && el.shadowLength >= minLength
        );
      }
    }, options);
  }

  /**
   * Listen for real-time events from browser
   */
  public onBrowserEvent(eventType: string, handler: (data: any) => void): () => void {
    const eventId = `browser-${eventType}-${Date.now()}`;
    this.browserEventListeners.set(eventId, handler);
    
    // Setup browser-side event forwarding
    this.setupBrowserEventListener(eventType, eventId);
    
    // Return cleanup function
    return () => {
      this.browserEventListeners.delete(eventId);
      this.removeBrowserEventListener(eventType, eventId);
    };
  }

  /**
   * Listen for widget state changes
   */
  public onWidgetChange(widgetName: string, handler: (widget: any) => void): () => void {
    return this.onBrowserEvent('widget-change', (data) => {
      if (data.widgetName === widgetName) {
        handler(data.widgetState);
      }
    });
  }

  /**
   * Listen for shadow DOM mutations
   */
  public onShadowDOMChange(selector: string, handler: (data: any) => void): () => void {
    return this.onBrowserEvent('shadow-dom-change', (data) => {
      if (data.selector === selector) {
        handler(data);
      }
    });
  }

  /**
   * Listen for performance threshold breaches
   */
  public onPerformanceIssue(threshold: number, handler: (data: any) => void): () => void {
    return this.onBrowserEvent('performance-issue', (data) => {
      if (data.score < threshold) {
        handler(data);
      }
    });
  }

  /**
   * Create a trigger that resolves when multiple conditions are met
   */
  public async waitForAll(conditions: JTAGTriggerCondition[], options?: JTAGWaitOptions): Promise<any[]> {
    const promises = conditions.map(condition => this.waitFor(condition, options));
    return Promise.all(promises);
  }

  /**
   * Create a trigger that resolves when any condition is met
   */
  public async waitForAny(conditions: JTAGTriggerCondition[], options?: JTAGWaitOptions): Promise<any> {
    const promises = conditions.map(condition => this.waitFor(condition, options));
    return Promise.race(promises);
  }

  /**
   * Create a timeout-based promise
   */
  public delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  /**
   * Create a promise that resolves after a certain number of successful probes
   */
  public async waitForStableCondition(
    condition: JTAGTriggerCondition,
    requiredSuccesses = 3,
    options?: JTAGWaitOptions
  ): Promise<any> {
    let successCount = 0;
    let lastResult: any;

    return this.waitFor({
      ...condition,
      name: `${condition.name}-stable`,
      condition: (data) => {
        if (condition.condition(data)) {
          successCount++;
          lastResult = data;
          return successCount >= requiredSuccesses;
        } else {
          successCount = 0;
          return false;
        }
      }
    }, options).then(() => lastResult);
  }

  /**
   * Execute a probe and return the result
   */
  private async executeProbe(method: JTAGProbeMethod): Promise<any> {
    // This would integrate with the existing command system
    // For now, return a mock result
    
    // TODO: Integration with browser probe system
    // const jsCode = `
    //   if (window.jtag) {
    //     const result = window.jtag.${method}();
    //     console.probe({
    //       message: "JTAG Event System Probe",
    //       category: "jtag-event-system",
    //       data: result,
    //       method: "${method}"
    //     });
    //     return result;
    //   } else {
    //     throw new Error('JTAG not available');
    //   }
    // `;

    // Execute in browser and parse result
    // This would be implemented using the existing console command
    return { success: true, data: {}, method };
  }

  /**
   * Setup bridge to receive events from browser
   */
  private setupBrowserEventBridge(): void {
    // Setup WebSocket or polling mechanism to receive browser events
    // This would integrate with the existing WebSocket system
    
    // Mock implementation for now
    this.emit('system-ready');
  }

  /**
   * Setup browser-side event listener
   */
  private setupBrowserEventListener(eventType: string, eventId: string): void {
    // TODO: Browser event listener setup
    // const jsCode = `
    //   // Setup browser-side event listener
    //   if (window.jtagEventSystem) {
    //     window.jtagEventSystem.on('${eventType}', function(data) {
    //       console.probe({
    //         message: "JTAG Event: ${eventType}",
    //         category: "jtag-events",
    //         eventType: "${eventType}",
    //         eventId: "${eventId}",
    //         data: data
    //       });
    //     });
    //   }
    // `;

    // Execute in browser
    // This would use the existing console command
    console.log(`Setting up browser event listener for ${eventType}:${eventId}`);
  }

  /**
   * Remove browser-side event listener
   */
  private removeBrowserEventListener(eventType: string, eventId: string): void {
    // TODO: Browser event listener removal
    // const jsCode = `
    //   if (window.jtagEventSystem) {
    //     window.jtagEventSystem.off('${eventType}', '${eventId}');
    //   }
    // `;

    // Execute in browser
    console.log(`Removing browser event listener for ${eventType}:${eventId}`);
  }

  /**
   * Cleanup waiter resources
   */
  private cleanup(waiterId: string): void {
    this.activeWaiters.delete(waiterId);
    
    const timer = this.retryTimers.get(waiterId);
    if (timer) {
      clearTimeout(timer);
      this.retryTimers.delete(waiterId);
    }

    const timeoutTimer = this.retryTimers.get(`${waiterId}-timeout`);
    if (timeoutTimer) {
      clearTimeout(timeoutTimer);
      this.retryTimers.delete(`${waiterId}-timeout`);
    }
  }

  /**
   * Cleanup all resources
   */
  public destroy(): void {
    // Clear all active waiters
    for (const [waiterId] of this.activeWaiters) {
      this.cleanup(waiterId);
    }

    // Clear all timers
    for (const [, timer] of this.retryTimers) {
      clearTimeout(timer);
    }
    this.retryTimers.clear();

    // Remove all listeners
    this.removeAllListeners();
    this.browserEventListeners.clear();
  }
}

// Pre-defined common conditions
export const JTAGConditions = {
  widgetsAllRendered: (): JTAGTriggerCondition => ({
    name: 'all-widgets-rendered',
    description: 'All widgets have rendered content in their shadow DOM',
    probe: 'widgets',
    condition: (data: WidgetAnalysisData) => 
      data.widgets.length > 0 && data.widgets.every(w => w.isRendered)
  }),

  sidebarRendered: (): JTAGTriggerCondition => ({
    name: 'sidebar-rendered',
    description: 'Sidebar widget has rendered content',
    probe: 'widgets',
    condition: (data: WidgetAnalysisData) =>
      data.widgets.some(w => w.name === 'continuum-sidebar' && w.isRendered)
  }),

  chatRendered: (): JTAGTriggerCondition => ({
    name: 'chat-rendered', 
    description: 'Chat widget has rendered content',
    probe: 'widgets',
    condition: (data: WidgetAnalysisData) =>
      data.widgets.some(w => w.name === 'chat-widget' && w.isRendered)
  }),

  systemHealthy: (minScore = 80): JTAGTriggerCondition => ({
    name: 'system-healthy',
    description: `System health score >= ${minScore}`,
    probe: 'health',
    condition: (data: HealthAnalysisData) =>
      data.overall === 'healthy' && data.score >= minScore
  }),

  performanceGood: (minScore = 70): JTAGTriggerCondition => ({
    name: 'performance-good',
    description: `Performance score >= ${minScore}`,
    probe: 'performance', 
    condition: (data) => data.overall.score >= minScore
  }),

  customElementsReady: (elements: string[]): JTAGTriggerCondition => ({
    name: 'custom-elements-ready',
    description: `Custom elements ${elements.join(', ')} are registered`,
    probe: 'customElements',
    condition: (data) => elements.every(name =>
      data.registry.some((reg: any) => reg.name === name && reg.defined)
    )
  })
};

// Global instance
export const jtagEvents = new JTAGEventSystem();