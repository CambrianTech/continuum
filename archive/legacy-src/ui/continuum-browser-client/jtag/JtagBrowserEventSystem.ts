/**
 * JTAG Browser Event System - Client-Side Promise-based Triggers
 * 
 * Browser-side event system that mirrors the CLI event system.
 * Provides reactive programming model with promises and event listeners.
 */

import '../types/console-extensions';
import {
  JTAGProbeMethod,
  WidgetAnalysisData,
  HealthAnalysisData
} from '../../../shared/types/JTAGSharedTypes';

export interface JTAGBrowserTriggerCondition {
  name: string;
  description: string;
  probe: JTAGProbeMethod;
  condition: (data: any) => boolean;
  timeout?: number;
  retryInterval?: number;
  maxRetries?: number;
}

export interface JTAGBrowserWaitOptions {
  timeout?: number;
  retryInterval?: number;
  maxRetries?: number;
  onRetry?: (attempt: number, data: any) => void;
  onProgress?: (data: any) => void;
}

export class JTAGBrowserEventSystem {
  private activeWaiters: Map<string, Promise<any>> = new Map();
  private retryTimers: Map<string, number> = new Map();
  private mutationObserver?: MutationObserver;
  private eventListeners: Map<string, (data: any) => void> = new Map();
  private isSetup = false;

  constructor() {
    this.setupDOMObservers();
  }

  /**
   * Wait for a condition to be met with promise-based resolution
   */
  public async waitFor<T = any>(
    condition: JTAGBrowserTriggerCondition,
    options: JTAGBrowserWaitOptions = {}
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
            console.probe({
              message: `✅ JTAG Condition Met: ${condition.name}`,
              category: 'jtag-events',
              data: { condition: condition.name, result: probeData }
            });
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
          const timer = setTimeout(checkCondition, retryInterval) as unknown as number;
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
      }, timeout) as unknown as number;

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
  public async waitForWidgetsRendered(
    widgetNames?: string[], 
    options?: JTAGBrowserWaitOptions
  ): Promise<WidgetAnalysisData> {
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
  public async waitForHealthy(minScore = 80, options?: JTAGBrowserWaitOptions): Promise<HealthAnalysisData> {
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
   * Wait for shadow DOM content to appear
   */
  public async waitForShadowContent(
    selector: string, 
    minLength = 10, 
    options?: JTAGBrowserWaitOptions
  ) {
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
   * Wait for element to exist in DOM
   */
  public async waitForElement(selector: string, options?: JTAGBrowserWaitOptions): Promise<Element> {
    return new Promise((resolve, reject) => {
      const { timeout = 10000 } = options || {};
      
      const element = document.querySelector(selector);
      if (element) {
        resolve(element);
        return;
      }

      const observer = new MutationObserver(() => {
        const element = document.querySelector(selector);
        if (element) {
          observer.disconnect();
          clearTimeout(timeoutTimer);
          resolve(element);
        }
      });

      const timeoutTimer = setTimeout(() => {
        observer.disconnect();
        reject(new Error(`Element '${selector}' not found within ${timeout}ms`));
      }, timeout);

      observer.observe(document.body, {
        childList: true,
        subtree: true
      });
    });
  }

  /**
   * Listen for DOM mutations on specific elements
   */
  public onElementChange(
    selector: string, 
    callback: (_mutations: MutationRecord[], element: Element) => void
  ): () => void {
    const observer = new MutationObserver((mutations) => {
      const element = document.querySelector(selector);
      if (element) {
        callback(mutations, element);
      }
    });

    const element = document.querySelector(selector);
    if (element) {
      observer.observe(element, {
        childList: true,
        subtree: true,
        attributes: true,
        characterData: true
      });
    }

    return () => observer.disconnect();
  }

  /**
   * Listen for shadow DOM changes
   */
  public onShadowDOMChange(
    selector: string,
    callback: (shadowRoot: ShadowRoot) => void
  ): () => void {
    return this.onElementChange(selector, (_mutations, element) => {
      const shadowRoot = (element as any).shadowRoot;
      if (shadowRoot) {
        callback(shadowRoot);
      }
    });
  }

  /**
   * Listen for widget state changes
   */
  public onWidgetStateChange(
    widgetName: string,
    callback: (widgetData: any) => void
  ): () => void {
    const checkInterval = setInterval(async () => {
      try {
        const widgetData = await this.executeProbe('widgets');
        const widget = widgetData.widgets.find((w: any) => w.name === widgetName);
        if (widget) {
          callback(widget);
        }
      } catch (error) {
        console.warn(`Error checking widget ${widgetName}:`, error);
      }
    }, 1000);

    return () => clearInterval(checkInterval);
  }

  /**
   * Create a simple event emitter for custom events
   */
  public emit(eventName: string, data: any): void {
    console.probe({
      message: `🔔 JTAG Event: ${eventName}`,
      category: 'jtag-browser-events',
      data: { eventName, ...data }
    });

    // Call registered listeners
    const listener = this.eventListeners.get(eventName);
    if (listener) {
      listener(data);
    }
  }

  /**
   * Listen for custom events
   */
  public on(eventName: string, callback: (data: any) => void): () => void {
    this.eventListeners.set(eventName, callback);
    return () => this.eventListeners.delete(eventName);
  }

  /**
   * Execute a probe and return the result
   */
  private async executeProbe(method: JTAGProbeMethod): Promise<any> {
    if (!window.jtag) {
      throw new Error('JTAG not available');
    }

    switch (method) {
      case 'widgets':
        return window.jtag.widgets({ autoLog: false }).data;
      case 'shadowDOM':
        return window.jtag.shadowDOM(undefined, { autoLog: false }).data;
      case 'customElements':
        return window.jtag.customElements({ autoLog: false }).data;
      case 'performance':
        return window.jtag.performance({ autoLog: false }).data;
      case 'network':
        return window.jtag.network({ autoLog: false }).data;
      case 'health':
        return window.jtag.health({ autoLog: false }).data;
      default:
        throw new Error(`Unknown probe method: ${method}`);
    }
  }

  /**
   * Setup DOM observers for automatic event emission
   */
  private setupDOMObservers(): void {
    if (this.isSetup) return;
    this.isSetup = true;

    // Watch for new widgets being added
    this.mutationObserver = new MutationObserver((mutations) => {
      mutations.forEach((mutation) => {
        mutation.addedNodes.forEach((node) => {
          if (node.nodeType === Node.ELEMENT_NODE) {
            const element = node as Element;
            if (element.tagName && element.tagName.includes('-')) {
              this.emit('widget-added', {
                tagName: element.tagName,
                element: element
              });
            }
          }
        });
      });
    });

    this.mutationObserver.observe(document.body, {
      childList: true,
      subtree: true
    });

    // Watch for widget render completions
    setInterval(async () => {
      try {
        const widgetData = await this.executeProbe('widgets');
        const renderedCount = widgetData.widgets.filter((w: any) => w.isRendered).length;
        
        if (renderedCount > 0) {
          this.emit('widgets-updated', {
            rendered: renderedCount,
            total: widgetData.widgets.length,
            widgets: widgetData.widgets
          });
        }
      } catch (error) {
        // Ignore errors during polling
      }
    }, 2000);
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

    // Disconnect observers
    if (this.mutationObserver) {
      this.mutationObserver.disconnect();
    }

    // Clear listeners
    this.eventListeners.clear();
  }
}

// Pre-defined browser conditions (same as CLI side)
export const JTAGBrowserConditions = {
  widgetsAllRendered: (): JTAGBrowserTriggerCondition => ({
    name: 'all-widgets-rendered',
    description: 'All widgets have rendered content in their shadow DOM',
    probe: 'widgets',
    condition: (data: WidgetAnalysisData) => 
      data.widgets.length > 0 && data.widgets.every(w => w.isRendered)
  }),

  sidebarRendered: (): JTAGBrowserTriggerCondition => ({
    name: 'sidebar-rendered',
    description: 'Sidebar widget has rendered content',
    probe: 'widgets',
    condition: (data: WidgetAnalysisData) =>
      data.widgets.some(w => w.name === 'continuum-sidebar' && w.isRendered)
  }),

  chatRendered: (): JTAGBrowserTriggerCondition => ({
    name: 'chat-rendered', 
    description: 'Chat widget has rendered content',
    probe: 'widgets',
    condition: (data: WidgetAnalysisData) =>
      data.widgets.some(w => w.name === 'chat-widget' && w.isRendered)
  }),

  systemHealthy: (minScore = 80): JTAGBrowserTriggerCondition => ({
    name: 'system-healthy',
    description: `System health score >= ${minScore}`,
    probe: 'health',
    condition: (data: HealthAnalysisData) =>
      data.overall === 'healthy' && data.score >= minScore
  }),

  customElementsReady: (elements: string[]): JTAGBrowserTriggerCondition => ({
    name: 'custom-elements-ready',
    description: `Custom elements ${elements.join(', ')} are registered`,
    probe: 'customElements',
    condition: (data) => elements.every(name =>
      data.registry.some((reg: any) => reg.name === name && reg.defined)
    )
  })
};

// Global instance
export const jtagBrowserEvents = new JTAGBrowserEventSystem();

// Add to window for easy access
declare global {
  interface Window {
    jtagEvents: JTAGBrowserEventSystem;
    JTAGConditions: typeof JTAGBrowserConditions;
  }
}

window.jtagEvents = jtagBrowserEvents;
window.JTAGConditions = JTAGBrowserConditions;