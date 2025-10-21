/**
 * JTAG Probe API - Compilation-Fixed Version
 * 
 * Provides strongly typed, chainable methods for probing widget states
 * with all TypeScript compilation errors resolved.
 */

import '../types/console-extensions';
import {
  JTAGProbeResponse,
  JTAGProbeOptions,
  JTAGProbeMethod,
  WidgetAnalysisData,
  ShadowDOMAnalysisData,
  CustomElementsAnalysisData,
  PerformanceAnalysisData,
  NetworkAnalysisData,
  HealthAnalysisData,
  ExecutionResult,
  WidgetState,
  WidgetSummary,
  WidgetIssue,
  ShadowDOMSummary,
  PerformanceScore,
  HealthSummary,
  JTAGBatchResponse,
  ExecutionRequest,
  WidgetLifecycleState,
  WidgetPerformanceData,
  HealthIssue,
  HealthWarning
} from '../../../shared/types/JTAGSharedTypes';

export class JTAGProbeAPIFixed {
  private lastProbeTime: Map<string, number> = new Map();
  private platform: 'browser' | 'server' | 'cli' = 'browser';

  constructor(platform: 'browser' | 'server' | 'cli' = 'browser') {
    this.platform = platform;
  }

  // Core JTAG API methods expected by browser interface
  getUUID(): any {
    return {
      uuid: 'browser_' + Date.now().toString(36),
      context: 'browser' as const,
      timestamp: new Date().toISOString(),
      sessionId: 'browser_session',
      processId: undefined,
      metadata: { platform: this.platform }
    };
  }

  log(component: string, message: string, data?: any): void {
    console.log(`[JTAG:${component}]`, message, data || '');
  }

  critical(component: string, message: string, data?: any): void {
    console.error(`[JTAG:CRITICAL:${component}]`, message, data || '');
  }

  async exec(code: string): Promise<any> {
    try {
      const result = eval(code);
      return {
        success: true,
        result,
        context: 'browser' as const,
        timestamp: new Date().toISOString(),
        executionTime: 0,
        uuid: 'exec_' + Date.now().toString(36)
      };
    } catch (error: any) {
      return {
        success: false,
        result: null,
        error: error.message,
        context: 'browser' as const,
        timestamp: new Date().toISOString(),
        executionTime: 0,
        uuid: 'exec_' + Date.now().toString(36)
      };
    }
  }

  async screenshot(filename: string, options?: any): Promise<any> {
    // Browser screenshot implementation would go here
    return {
      success: true,
      filepath: `/screenshots/${filename}`,
      filename,
      context: 'browser' as const,
      timestamp: new Date().toISOString(),
      options,
      metadata: {
        width: window.innerWidth,
        height: window.innerHeight,
        size: 0,
        selector: options?.selector
      }
    };
  }

  /**
   * Comprehensive widget state analysis
   */
  public widgets(options: JTAGProbeOptions = {}): JTAGProbeResponse<WidgetAnalysisData> {
    return this.executeProbe(() => {
      const widgetElements = Array.from(document.querySelectorAll('continuum-sidebar, chat-widget'));
      
      const widgets: WidgetState[] = widgetElements.map(element => {
        const shadowRoot = (element as any).shadowRoot;
        const shadowContent = shadowRoot?.innerHTML || '';
        
        return {
          name: element.tagName.toLowerCase().replace('-widget', ''),
          tagName: element.tagName,
          exists: true,
          hasShadowRoot: !!shadowRoot,
          shadowContentLength: shadowContent.length,
          shadowContentPreview: shadowContent.substring(0, 200),
          isRendered: shadowContent.trim().length > 0,
          hasStyles: !!shadowRoot?.querySelector('style'),
          styleCount: shadowRoot?.querySelectorAll('style').length || 0,
          lifecycle: this.analyzeWidgetLifecycle(element),
          performance: this.analyzeWidgetPerformance(element),
          errors: []
        };
      });

      const summary: WidgetSummary = {
        total: widgets.length,
        rendered: widgets.filter(w => w.isRendered).length,
        broken: widgets.filter(w => !w.isRendered || w.errors.length > 0).length,
        empty: widgets.filter(w => !w.isRendered).length,
        performance: this.calculatePerformanceGrade(widgets)
      };

      const issues: WidgetIssue[] = widgets
        .filter(w => !w.isRendered || !w.hasStyles)
        .map(w => ({
          widget: w.name,
          type: !w.isRendered ? 'empty-shadow' : 'missing-styles',
          severity: 'error' as const,
          message: !w.isRendered ? 'Widget shadow DOM is empty' : 'Widget has no CSS styles',
          suggestion: !w.isRendered ? 'Check widget render() method' : 'Verify CSS loading'
        }));

      return { widgets, summary, issues };
    }, 'widgets', options);
  }

  /**
   * Shadow DOM deep analysis
   */
  public shadowDOM(selector?: string, options: JTAGProbeOptions = {}): JTAGProbeResponse<ShadowDOMAnalysisData> {
    return this.executeProbe(() => {
      const elements = selector ? 
        document.querySelectorAll(selector) : 
        document.querySelectorAll('continuum-sidebar, chat-widget');
      
      const shadowElements = Array.from(elements).map(element => {
        const shadowRoot = (element as any).shadowRoot;
        const shadowHTML = shadowRoot?.innerHTML || null;
        
        return {
          selector: element.tagName.toLowerCase(),
          tagName: element.tagName,
          hasShadowRoot: !!shadowRoot,
          shadowHTML,
          shadowLength: shadowHTML?.length || 0,
          childCount: shadowRoot?.children?.length || 0,
          hasContent: (shadowHTML?.trim().length || 0) > 0,
          styles: Array.from(shadowRoot?.querySelectorAll('style') || []).map((style) => ({
            source: 'injected' as const,
            length: (style as HTMLStyleElement).textContent?.length || 0,
            rules: (style as HTMLStyleElement).sheet?.cssRules?.length || 0,
            isBaseCSS: (style as HTMLStyleElement).textContent?.includes('BaseWidget') || false
          }))
        };
      });

      const summary: ShadowDOMSummary = {
        totalElements: shadowElements.length,
        withShadowRoot: shadowElements.filter(e => e.hasShadowRoot).length,
        withContent: shadowElements.filter(e => e.hasContent).length,
        totalStyles: shadowElements.reduce((sum, e) => sum + e.styles.length, 0)
      };

      return { elements: shadowElements, summary };
    }, 'shadow-dom', options);
  }

  /**
   * Custom element registration analysis
   */
  public customElements(options: JTAGProbeOptions = {}): JTAGProbeResponse<CustomElementsAnalysisData> {
    return this.executeProbe(() => {
      const supported = !!window.customElements;
      
      const elementNames = ['continuum-sidebar', 'chat-widget'];
      const registry = elementNames.map(name => ({
        name,
        defined: window.customElements?.get(name) !== undefined,
        constructor: typeof window.customElements?.get(name) || 'undefined'
      }));

      const instances = elementNames.map(name => ({
        name,
        count: document.querySelectorAll(name).length,
        rendered: Array.from(document.querySelectorAll(name))
          .filter(el => (el as any).shadowRoot?.innerHTML?.trim()).length,
        broken: Array.from(document.querySelectorAll(name))
          .filter(el => !(el as any).shadowRoot?.innerHTML?.trim()).length
      }));

      const summary = {
        totalDefinitions: registry.filter(r => r.defined).length,
        totalInstances: instances.reduce((sum, i) => sum + i.count, 0),
        workingInstances: instances.reduce((sum, i) => sum + i.rendered, 0),
        registrationHealth: registry.every(r => r.defined) ? 'good' as const : 
                          registry.some(r => r.defined) ? 'partial' as const : 'broken' as const
      };

      return { supported, registry, instances, summary };
    }, 'custom-elements', options);
  }

  /**
   * Performance metrics analysis
   */
  public performance(options: JTAGProbeOptions = {}): JTAGProbeResponse<PerformanceAnalysisData> {
    return this.executeProbe(() => {
      const perfMemory = (performance as any).memory;
      const memory = perfMemory ? {
        used: Math.round(perfMemory.usedJSHeapSize / 1024 / 1024),
        total: Math.round(perfMemory.totalJSHeapSize / 1024 / 1024),
        limit: Math.round(perfMemory.jsHeapSizeLimit / 1024 / 1024),
        percentage: Math.round((perfMemory.usedJSHeapSize / perfMemory.jsHeapSizeLimit) * 100)
      } : null;

      const timing = {
        loadComplete: performance.now(),
        domComplete: Date.now(),
        renderTime: performance.now()
      };

      const resources = {
        scripts: document.scripts.length,
        stylesheets: document.styleSheets.length,
        images: document.images.length,
        totalSize: 0,
        loadTime: performance.now()
      };

      const widgets = document.querySelectorAll('continuum-sidebar, chat-widget');
      const renderedWidgets = Array.from(widgets).filter(w => 
        (w as any).shadowRoot?.innerHTML?.trim()
      );

      const widgetMetrics = {
        totalWidgets: widgets.length,
        renderedWidgets: renderedWidgets.length,
        averageRenderTime: 0,
        memoryPerWidget: memory ? memory.used / Math.max(widgets.length, 1) : 0
      };

      const score = this.calculatePerformanceScore(memory, widgetMetrics);
      const overall: PerformanceScore = {
        score,
        grade: score >= 90 ? 'A' : score >= 80 ? 'B' : score >= 70 ? 'C' : score >= 60 ? 'D' : 'F',
        issues: [],
        recommendations: []
      };

      return { memory, timing, resources, widgets: widgetMetrics, overall };
    }, 'performance', options);
  }

  /**
   * Network and API connectivity analysis
   */
  public network(options: JTAGProbeOptions = {}): JTAGProbeResponse<NetworkAnalysisData> {
    return this.executeProbe(() => {
      const continuum = {
        available: typeof window.continuum !== 'undefined',
        state: window.continuum?.state || 'unknown',
        sessionId: window.continuum?.sessionId || undefined,
        clientId: window.continuum?.clientId || undefined,
        methods: Object.keys(window.continuum || {}).filter(k => {
          const obj = window.continuum as any;
          return obj && typeof obj[k] === 'function';
        }),
        version: window.continuum?.version || undefined
      };

      const websocket = {
        supported: typeof WebSocket !== 'undefined',
        connected: false,
        readyState: 0,
        messagesReceived: 0,
        messagesSent: 0
      };

      const latency = {
        ping: undefined,
        apiResponseTime: undefined,
        websocketLatency: undefined
      };

      const connectivity = {
        status: navigator.onLine ? 'good' as const : 'offline' as const,
        issues: []
      };

      return {
        online: navigator.onLine,
        continuum,
        websocket,
        latency,
        connectivity
      };
    }, 'network', options);
  }

  /**
   * Comprehensive health check
   */
  public health(options: JTAGProbeOptions = {}): JTAGProbeResponse<HealthAnalysisData> {
    return this.executeProbe(() => {
      const widgetData = this.widgets({ autoLog: false }).data;
      const customElementsData = this.customElements({ autoLog: false }).data;
      const performanceData = this.performance({ autoLog: false }).data;
      const networkData = this.network({ autoLog: false }).data;

      const components = [
        {
          name: 'widgets',
          status: widgetData.summary.broken === 0 ? 'healthy' as const : 'issues-detected' as const,
          message: `${widgetData.summary.rendered}/${widgetData.summary.total} widgets rendered`
        },
        {
          name: 'custom-elements',
          status: customElementsData.summary.registrationHealth === 'good' ? 'healthy' as const : 'issues-detected' as const,
          message: `${customElementsData.summary.totalDefinitions} elements registered`
        },
        {
          name: 'performance',
          status: performanceData.overall.score >= 70 ? 'healthy' as const : 'issues-detected' as const,
          message: `Performance score: ${performanceData.overall.score}`
        },
        {
          name: 'network',
          status: networkData.continuum.available ? 'healthy' as const : 'critical' as const,
          message: networkData.online ? 'Connected' : 'Offline'
        }
      ];

      const issues: HealthIssue[] = widgetData.issues.map(issue => ({
        component: 'widgets',
        type: issue.type,
        severity: issue.severity,
        message: issue.message,
        suggestion: issue.suggestion,
        fixable: true
      }));

      const warnings: HealthWarning[] = [];

      const summary: HealthSummary = {
        widgets: {
          status: widgetData.summary.broken === 0 ? 'healthy' as const : 'issues-detected' as const,
          score: Math.round((widgetData.summary.rendered / widgetData.summary.total) * 100)
        },
        performance: {
          status: performanceData.overall.score >= 70 ? 'healthy' as const : 'issues-detected' as const,
          score: performanceData.overall.score
        },
        network: {
          status: networkData.continuum.available ? 'healthy' as const : 'critical' as const,
          score: networkData.online ? 100 : 0
        },
        memory: {
          status: (performanceData.memory?.percentage || 0) < 80 ? 'healthy' as const : 'issues-detected' as const,
          score: Math.max(0, 100 - (performanceData.memory?.percentage || 0))
        }
      };

      const overallScore = Math.round(
        (summary.widgets.score + summary.performance.score + 
         summary.network.score + summary.memory.score) / 4
      );

      const overall = issues.length === 0 ? 'healthy' as const : 
                     issues.some(i => i.severity === 'critical') ? 'critical' as const : 'issues-detected' as const;

      return {
        overall,
        components,
        issues,
        warnings,
        summary,
        score: overallScore,
        recommendations: this.generateRecommendations(issues)
      };
    }, 'health-check', options);
  }

  /**
   * Execute custom JavaScript with strong typing
   */
  public execute(request: ExecutionRequest, options: JTAGProbeOptions = {}): JTAGProbeResponse<ExecutionResult> {
    return this.executeProbe(() => {
      const startTime = performance.now();
      const memoryBefore = (performance as any).memory?.usedJSHeapSize;

      try {
        const result = eval(request.code);
        const executionTime = performance.now() - startTime;
        const memoryAfter = (performance as any).memory?.usedJSHeapSize;

        return {
          success: true,
          result,
          code: request.code,
          executionTime,
          memoryBefore: memoryBefore ? Math.round(memoryBefore / 1024 / 1024) : undefined,
          memoryAfter: memoryAfter ? Math.round(memoryAfter / 1024 / 1024) : undefined,
          context: this.platform
        };
      } catch (error) {
        const executionTime = performance.now() - startTime;

        return {
          success: false,
          error: error instanceof Error ? error.message : String(error),
          code: request.code,
          executionTime,
          memoryBefore: memoryBefore ? Math.round(memoryBefore / 1024 / 1024) : undefined,
          context: this.platform
        };
      }
    }, 'custom-execute', options);
  }

  /**
   * Batch multiple probes with strong typing
   */
  public batch(methods: JTAGProbeMethod[], options: JTAGProbeOptions = {}): JTAGBatchResponse {
    const startTime = performance.now();
    const results: Record<string, JTAGProbeResponse> = {};
    const errors = [];

    for (const method of methods) {
      try {
        switch (method) {
          case 'widgets':
            results[method] = this.widgets({ ...options, autoLog: false });
            break;
          case 'shadowDOM':
            results[method] = this.shadowDOM(undefined, { ...options, autoLog: false });
            break;
          case 'customElements':
            results[method] = this.customElements({ ...options, autoLog: false });
            break;
          case 'performance':
            results[method] = this.performance({ ...options, autoLog: false });
            break;
          case 'network':
            results[method] = this.network({ ...options, autoLog: false });
            break;
          case 'health':
            results[method] = this.health({ ...options, autoLog: false });
            break;
          default:
            errors.push({
              type: 'probe-error' as const,
              message: `Unknown probe method: ${method}`,
              timestamp: Date.now(),
              platform: this.platform
            });
        }
      } catch (error) {
        errors.push({
          type: 'probe-error' as const,
          message: `Error executing ${method}: ${error instanceof Error ? error.message : String(error)}`,
          timestamp: Date.now(),
          platform: this.platform
        });
      }
    }

    const executionTime = performance.now() - startTime;

    return { results, executionTime, errors };
  }

  /**
   * Core probe execution with comprehensive error handling
   */
  private executeProbe<T>(
    probeFunction: () => T,
    category: string,
    options: JTAGProbeOptions
  ): JTAGProbeResponse<T> {
    const startTime = performance.now();
    const { throttle = 0, autoLog = true } = options;
    const probeKey = `${category}-${JSON.stringify(options)}`;
    
    // Check throttling
    if (throttle > 0) {
      const lastTime = this.lastProbeTime.get(probeKey) || 0;
      const now = Date.now();
      if (now - lastTime < throttle) {
        return {
          success: false,
          data: { throttled: true, remainingTime: throttle - (now - lastTime) } as any,
          timestamp: now,
          category: options.category || category,
          executionTime: performance.now() - startTime,
          error: {
            type: 'probe-error',
            message: 'Probe throttled',
            timestamp: now,
            platform: this.platform
          }
        };
      }
      this.lastProbeTime.set(probeKey, now);
    }

    try {
      const data = probeFunction();
      const executionTime = performance.now() - startTime;
      
      const result: JTAGProbeResponse<T> = {
        success: true,
        data,
        timestamp: Date.now(),
        category: options.category || category,
        executionTime,
        metadata: {
          platform: this.platform,
          version: '1.0.0'
        }
      };

      // Auto-log if enabled
      if (autoLog) {
        console.probe({
          message: `🔍 JTAG Probe: ${category}`,
          category: result.category,
          tags: options.tags || [category],
          data: result.data
        });
      }

      return result;
    } catch (error) {
      const executionTime = performance.now() - startTime;
      
      const result: JTAGProbeResponse<T> = {
        success: false,
        data: {} as T,
        timestamp: Date.now(),
        category: options.category || category,
        executionTime,
        error: {
          type: 'probe-error',
          message: error instanceof Error ? error.message : String(error),
          stack: error instanceof Error ? error.stack : undefined,
          timestamp: Date.now(),
          platform: this.platform
        }
      };

      if (autoLog) {
        console.probe({
          message: `❌ JTAG Probe Error: ${category}`,
          category: result.category,
          tags: ['error', ...(options.tags || [])],
          data: { error: result.error }
        });
      }

      return result;
    }
  }

  // Helper methods
  private analyzeWidgetLifecycle(element: Element): WidgetLifecycleState {
    return {
      constructed: true,
      connected: element.isConnected,
      rendered: !!(element as any).shadowRoot?.innerHTML?.trim(),
      styled: !!(element as any).shadowRoot?.querySelector('style'),
      interactive: true,
      timestamp: Date.now()
    };
  }

  private analyzeWidgetPerformance(element: Element): WidgetPerformanceData {
    return {
      renderTime: undefined,
      memoryUsage: undefined,
      cssLoadTime: undefined,
      domComplexity: (element as any).shadowRoot?.querySelectorAll('*').length || 0
    };
  }

  private calculatePerformanceGrade(widgets: WidgetState[]): 'good' | 'fair' | 'poor' {
    const renderedRatio = widgets.filter(w => w.isRendered).length / Math.max(widgets.length, 1);
    return renderedRatio >= 0.8 ? 'good' : renderedRatio >= 0.5 ? 'fair' : 'poor';
  }

  private calculatePerformanceScore(memory: any, widgets: any): number {
    let score = 100;
    
    if (memory && memory.percentage > 80) score -= 20;
    else if (memory && memory.percentage > 60) score -= 10;
    
    const renderRatio = widgets.renderedWidgets / Math.max(widgets.totalWidgets, 1);
    if (renderRatio < 0.5) score -= 30;
    else if (renderRatio < 0.8) score -= 15;
    
    return Math.max(0, Math.round(score));
  }

  private generateRecommendations(issues: HealthIssue[]): string[] {
    const recommendations: string[] = [];
    
    if (issues.some(i => i.type === 'empty-shadow')) {
      recommendations.push('Check widget render() methods for proper shadow DOM content generation');
    }
    
    if (issues.some(i => i.type === 'missing-styles')) {
      recommendations.push('Verify CSS loading and injection into widget shadow DOM');
    }
    
    return recommendations;
  }
}

// Create global instance with strong typing
export const jtag = new JTAGProbeAPIFixed('browser');

// Add to window for easy access
declare global {
  interface Window {
    jtag: JTAGProbeAPIFixed;
  }
}

window.jtag = jtag;