/**
 * JTAG Probe API Integration Tests
 * 
 * Tests the strongly typed JTAG API against real browser environment
 */

import { JTAGProbeAPITyped } from '../../JTAGProbeAPITyped';
import {
  WidgetAnalysisData,
  ShadowDOMAnalysisData,
  CustomElementsAnalysisData,
  PerformanceAnalysisData,
  NetworkAnalysisData,
  HealthAnalysisData,
  ExecutionResult,
  JTAGProbeResponse,
  HealthStatus,
  JTAG_PROBE_METHODS,
  isJTAGProbeMethod,
  isHealthStatus
} from '../../../../shared/types/JTAGSharedTypes';

describe('JTAGProbeAPI Integration Tests', () => {
  let jtag: JTAGProbeAPITyped;

  beforeAll(() => {
    // Setup mock browser environment
    setupMockBrowserEnvironment();
    jtag = new JTAGProbeAPITyped('browser');
  });

  describe('Type Safety', () => {
    test('should have strongly typed return values', () => {
      const widgetResult = jtag.widgets();
      
      // Type assertions to ensure compilation
      expect(widgetResult.success).toBeDefined();
      expect(widgetResult.data).toBeDefined();
      expect(widgetResult.timestamp).toBeDefined();
      expect(widgetResult.category).toBeDefined();
      expect(widgetResult.executionTime).toBeDefined();
      
      if (widgetResult.success) {
        const data: WidgetAnalysisData = widgetResult.data;
        expect(Array.isArray(data.widgets)).toBe(true);
        expect(data.summary).toBeDefined();
        expect(data.issues).toBeDefined();
        
        data.widgets.forEach(widget => {
          expect(typeof widget.name).toBe('string');
          expect(typeof widget.exists).toBe('boolean');
          expect(typeof widget.hasShadowRoot).toBe('boolean');
          expect(typeof widget.isRendered).toBe('boolean');
        });
      }
    });

    test('should validate JTAG probe methods', () => {
      expect(isJTAGProbeMethod('widgets')).toBe(true);
      expect(isJTAGProbeMethod('invalidMethod')).toBe(false);
      
      JTAG_PROBE_METHODS.forEach(method => {
        expect(isJTAGProbeMethod(method)).toBe(true);
      });
    });

    test('should validate health status values', () => {
      expect(isHealthStatus('healthy')).toBe(true);
      expect(isHealthStatus('invalid')).toBe(false);
    });
  });

  describe('Cross-Platform Data Serialization', () => {
    test('widget analysis data should be JSON serializable', () => {
      const result = jtag.widgets({ autoLog: false });
      
      if (result.success) {
        const serialized = JSON.stringify(result.data);
        const deserialized: WidgetAnalysisData = JSON.parse(serialized);
        
        expect(deserialized.widgets).toEqual(result.data.widgets);
        expect(deserialized.summary).toEqual(result.data.summary);
        expect(deserialized.issues).toEqual(result.data.issues);
      }
    });

    test('shadow DOM analysis should be serializable', () => {
      const result = jtag.shadowDOM(undefined, { autoLog: false });
      
      if (result.success) {
        const serialized = JSON.stringify(result.data);
        const deserialized: ShadowDOMAnalysisData = JSON.parse(serialized);
        
        expect(deserialized.elements).toEqual(result.data.elements);
        expect(deserialized.summary).toEqual(result.data.summary);
      }
    });

    test('health analysis should be fully serializable', () => {
      const result = jtag.health({ autoLog: false });
      
      if (result.success) {
        const serialized = JSON.stringify(result.data);
        const deserialized: HealthAnalysisData = JSON.parse(serialized);
        
        expect(deserialized.overall).toEqual(result.data.overall);
        expect(deserialized.components).toEqual(result.data.components);
        expect(deserialized.summary).toEqual(result.data.summary);
        expect(deserialized.score).toEqual(result.data.score);
      }
    });
  });

  describe('Real Browser Integration', () => {
    test('should detect actual widget states', () => {
      const result = jtag.widgets();
      
      expect(result.success).toBe(true);
      expect(result.data.widgets).toBeDefined();
      expect(result.data.summary.total).toBeGreaterThanOrEqual(0);
      
      // Check widget state structure
      result.data.widgets.forEach(widget => {
        expect(widget).toMatchObject({
          name: expect.any(String),
          tagName: expect.any(String),
          exists: expect.any(Boolean),
          hasShadowRoot: expect.any(Boolean),
          shadowContentLength: expect.any(Number),
          shadowContentPreview: expect.any(String),
          isRendered: expect.any(Boolean),
          hasStyles: expect.any(Boolean),
          styleCount: expect.any(Number),
          lifecycle: expect.objectContaining({
            constructed: expect.any(Boolean),
            connected: expect.any(Boolean),
            rendered: expect.any(Boolean),
            styled: expect.any(Boolean),
            interactive: expect.any(Boolean),
            timestamp: expect.any(Number)
          }),
          performance: expect.objectContaining({
            domComplexity: expect.any(Number)
          }),
          errors: expect.any(Array)
        });
      });
    });

    test('should analyze shadow DOM correctly', () => {
      const result = jtag.shadowDOM();
      
      expect(result.success).toBe(true);
      expect(result.data.elements).toBeDefined();
      expect(result.data.summary).toBeDefined();
      
      result.data.elements.forEach(element => {
        expect(element).toMatchObject({
          selector: expect.any(String),
          tagName: expect.any(String),
          hasShadowRoot: expect.any(Boolean),
          shadowLength: expect.any(Number),
          childCount: expect.any(Number),
          hasContent: expect.any(Boolean),
          styles: expect.any(Array)
        });
      });
    });

    test('should check custom elements properly', () => {
      const result = jtag.customElements();
      
      expect(result.success).toBe(true);
      expect(result.data.supported).toBe(true); // Should be true in test environment
      expect(result.data.registry).toBeDefined();
      expect(result.data.instances).toBeDefined();
      expect(result.data.summary).toBeDefined();
      
      expect(result.data.summary).toMatchObject({
        totalDefinitions: expect.any(Number),
        totalInstances: expect.any(Number),
        workingInstances: expect.any(Number),
        registrationHealth: expect.stringMatching(/^(good|partial|broken)$/)
      });
    });

    test('should gather performance metrics', () => {
      const result = jtag.performance();
      
      expect(result.success).toBe(true);
      expect(result.data.timing).toBeDefined();
      expect(result.data.resources).toBeDefined();
      expect(result.data.widgets).toBeDefined();
      expect(result.data.overall).toBeDefined();
      
      expect(result.data.overall.score).toBeGreaterThanOrEqual(0);
      expect(result.data.overall.score).toBeLessThanOrEqual(100);
      expect(['A', 'B', 'C', 'D', 'F']).toContain(result.data.overall.grade);
    });

    test('should analyze network connectivity', () => {
      const result = jtag.network();
      
      expect(result.success).toBe(true);
      expect(result.data.online).toBeDefined();
      expect(result.data.continuum).toBeDefined();
      expect(result.data.websocket).toBeDefined();
      expect(result.data.connectivity).toBeDefined();
      
      expect(result.data.continuum).toMatchObject({
        available: expect.any(Boolean),
        state: expect.any(String),
        methods: expect.any(Array)
      });
    });

    test('should perform comprehensive health check', () => {
      const result = jtag.health();
      
      expect(result.success).toBe(true);
      expect(['healthy', 'issues-detected', 'critical', 'unknown']).toContain(result.data.overall);
      expect(result.data.components).toBeDefined();
      expect(result.data.summary).toBeDefined();
      expect(result.data.score).toBeGreaterThanOrEqual(0);
      expect(result.data.score).toBeLessThanOrEqual(100);
      
      // Check component health structure
      result.data.components.forEach(component => {
        expect(component).toMatchObject({
          name: expect.any(String),
          status: expect.stringMatching(/^(healthy|issues-detected|critical|unknown)$/),
          message: expect.any(String)
        });
      });
      
      // Check summary structure
      Object.values(result.data.summary).forEach(summary => {
        expect(summary).toMatchObject({
          status: expect.stringMatching(/^(healthy|issues-detected|critical|unknown)$/),
          score: expect.any(Number)
        });
      });
    });
  });

  describe('Batch Operations', () => {
    test('should execute multiple probes correctly', () => {
      const methods = ['widgets', 'customElements', 'performance'] as const;
      const result = jtag.batch(methods);
      
      expect(result.executionTime).toBeGreaterThan(0);
      expect(result.errors).toBeDefined();
      
      methods.forEach(method => {
        expect(result.results[method]).toBeDefined();
        expect(result.results[method].success).toBeDefined();
        expect(result.results[method].timestamp).toBeDefined();
      });
    });

    test('should handle invalid methods gracefully', () => {
      const methods = ['widgets', 'invalidMethod' as any];
      const result = jtag.batch(methods);
      
      expect(result.errors.length).toBeGreaterThan(0);
      expect(result.results.widgets).toBeDefined();
      expect(result.results.widgets.success).toBe(true);
    });
  });

  describe('Custom Execution', () => {
    test('should execute JavaScript with proper typing', () => {
      const request = { code: 'document.title', context: 'browser' as const };
      const result = jtag.execute(request);
      
      expect(result.success).toBe(true);
      if (result.success) {
        const execResult: ExecutionResult = result.data;
        expect(execResult.success).toBe(true);
        expect(execResult.code).toBe('document.title');
        expect(execResult.executionTime).toBeGreaterThan(0);
        expect(execResult.context).toBe('browser');
      }
    });

    test('should handle execution errors properly', () => {
      const request = { code: 'throw new Error("test error")', context: 'browser' as const };
      const result = jtag.execute(request);
      
      expect(result.success).toBe(true);
      if (result.success) {
        const execResult: ExecutionResult = result.data;
        expect(execResult.success).toBe(false);
        expect(execResult.error).toContain('test error');
      }
    });
  });

  describe('Platform Compatibility', () => {
    test('should identify correct platform', () => {
      const browserJtag = new JTAGProbeAPITyped('browser');
      const serverJtag = new JTAGProbeAPITyped('server');
      const cliJtag = new JTAGProbeAPITyped('cli');
      
      expect(browserJtag).toBeDefined();
      expect(serverJtag).toBeDefined();
      expect(cliJtag).toBeDefined();
    });

    test('should include platform metadata', () => {
      const result = jtag.widgets();
      
      expect(result.metadata).toBeDefined();
      expect(result.metadata?.platform).toBe('browser');
      expect(result.metadata?.version).toBeDefined();
    });
  });
});

// Helper function to setup mock browser environment
function setupMockBrowserEnvironment() {
  // Mock DOM elements
  const mockSidebar = document.createElement('continuum-sidebar');
  const mockChat = document.createElement('chat-widget');
  
  // Mock shadow roots
  Object.defineProperty(mockSidebar, 'shadowRoot', {
    value: {
      innerHTML: '<div class="sidebar-content">Sidebar content</div>',
      querySelector: jest.fn(),
      querySelectorAll: jest.fn(() => [])
    }
  });
  
  Object.defineProperty(mockChat, 'shadowRoot', {
    value: {
      innerHTML: '<div class="chat-content">Chat content</div>',
      querySelector: jest.fn(),
      querySelectorAll: jest.fn(() => [])
    }
  });
  
  // Mock document.querySelectorAll
  const originalQuerySelectorAll = document.querySelectorAll;
  document.querySelectorAll = jest.fn((selector: string) => {
    if (selector.includes('continuum-sidebar') || selector.includes('chat-widget')) {
      return [mockSidebar, mockChat] as any;
    }
    return originalQuerySelectorAll.call(document, selector);
  });
  
  // Mock window.customElements
  Object.defineProperty(window, 'customElements', {
    value: {
      get: jest.fn((name: string) => {
        if (name === 'continuum-sidebar' || name === 'chat-widget') {
          return function() {};
        }
        return undefined;
      })
    }
  });
  
  // Mock window.continuum
  Object.defineProperty(window, 'continuum', {
    value: {
      state: 'connected',
      sessionId: 'test-session',
      clientId: 'test-client',
      version: '1.0.0',
      execute: jest.fn(),
      probe: jest.fn()
    }
  });
  
  // Mock performance.memory
  Object.defineProperty(performance, 'memory', {
    value: {
      usedJSHeapSize: 10 * 1024 * 1024,
      totalJSHeapSize: 20 * 1024 * 1024,
      jsHeapSizeLimit: 100 * 1024 * 1024
    }
  });
  
  // Mock console.probe
  Object.defineProperty(console, 'probe', {
    value: jest.fn()
  });
}