/**
 * JTAG Probe API Unit Tests
 */

import { JTAGProbeAPI } from '../../JTAGProbeAPI';

// Mock DOM environment for testing
const mockElement = {
  tagName: 'CONTINUUM-SIDEBAR',
  shadowRoot: {
    innerHTML: '<div class="widget-content">Test content</div>',
    querySelector: jest.fn(),
    querySelectorAll: jest.fn(() => [])
  }
};

const mockDocument = {
  querySelectorAll: jest.fn(() => [mockElement]),
  querySelector: jest.fn(() => mockElement),
  styleSheets: { length: 3 },
  scripts: { length: 2 },
  images: { length: 5 }
};

const mockWindow = {
  customElements: {
    get: jest.fn((name: string) => name === 'continuum-sidebar' ? function() {} : undefined)
  },
  continuum: {
    state: 'connected',
    sessionId: 'test-session',
    clientId: 'test-client'
  },
  performance: {
    now: () => 1000,
    memory: {
      usedJSHeapSize: 10 * 1024 * 1024,
      totalJSHeapSize: 20 * 1024 * 1024,
      jsHeapSizeLimit: 100 * 1024 * 1024
    }
  },
  navigator: {
    onLine: true
  }
};

// Mock console.probe
const mockConsoleProbe = jest.fn();

// Setup global mocks
beforeEach(() => {
  global.document = mockDocument as any;
  global.window = mockWindow as any;
  global.console = { ...console, probe: mockConsoleProbe };
  global.getComputedStyle = jest.fn(() => ({ display: 'block', visibility: 'visible' }));
  
  jest.clearAllMocks();
});

describe('JTAGProbeAPI', () => {
  let jtag: JTAGProbeAPI;

  beforeEach(() => {
    jtag = new JTAGProbeAPI();
  });

  describe('widgets()', () => {
    test('should analyze widget states correctly', () => {
      const result = jtag.widgets();
      
      expect(result.success).toBe(true);
      expect(result.data).toHaveLength(1);
      expect(result.data[0]).toMatchObject({
        name: 'continuum-sidebar',
        exists: true,
        hasShadowRoot: true,
        isRendered: true
      });
      expect(result.category).toBe('widgets');
    });

    test('should handle empty widget list', () => {
      mockDocument.querySelectorAll.mockReturnValue([]);
      
      const result = jtag.widgets();
      
      expect(result.success).toBe(true);
      expect(result.data).toHaveLength(0);
    });
  });

  describe('shadowDOM()', () => {
    test('should analyze shadow DOM content', () => {
      const result = jtag.shadowDOM();
      
      expect(result.success).toBe(true);
      expect(result.data).toHaveLength(1);
      expect(result.data[0]).toMatchObject({
        selector: 'continuum-sidebar',
        hasShadowRoot: true,
        shadowLength: expect.any(Number)
      });
    });

    test('should handle custom selector', () => {
      const result = jtag.shadowDOM('chat-widget');
      
      expect(mockDocument.querySelectorAll).toHaveBeenCalledWith('chat-widget');
      expect(result.success).toBe(true);
    });
  });

  describe('customElements()', () => {
    test('should check custom element registration', () => {
      const result = jtag.customElements();
      
      expect(result.success).toBe(true);
      expect(result.data).toMatchObject({
        supported: true,
        sidebarRegistered: 'function',
        chatRegistered: 'undefined'
      });
    });
  });

  describe('performance()', () => {
    test('should collect performance metrics', () => {
      const result = jtag.performance();
      
      expect(result.success).toBe(true);
      expect(result.data).toMatchObject({
        memory: {
          used: 10,
          total: 20,
          limit: 100
        },
        timing: {
          loadComplete: 1000
        }
      });
    });
  });

  describe('network()', () => {
    test('should check network and API status', () => {
      const result = jtag.network();
      
      expect(result.success).toBe(true);
      expect(result.data).toMatchObject({
        online: true,
        continuum: {
          available: true,
          state: 'connected',
          sessionId: 'test-session'
        }
      });
    });
  });

  describe('execute()', () => {
    test('should execute custom JavaScript', () => {
      const result = jtag.execute('2 + 2');
      
      expect(result.success).toBe(true);
      expect(result.data.result).toBe(4);
      expect(result.data.code).toBe('2 + 2');
    });

    test('should handle JavaScript errors', () => {
      const result = jtag.execute('throw new Error("test error")');
      
      expect(result.success).toBe(true);
      expect(result.data.error).toBe('test error');
    });
  });

  describe('health()', () => {
    test('should perform comprehensive health check', () => {
      const result = jtag.health();
      
      expect(result.success).toBe(true);
      expect(result.data).toHaveProperty('overall');
      expect(result.data).toHaveProperty('issues');
      expect(result.data).toHaveProperty('summary');
    });

    test('should detect rendering issues', () => {
      // Mock widget with empty shadow DOM
      mockElement.shadowRoot.innerHTML = '';
      
      const result = jtag.health();
      
      expect(result.data.overall).toBe('issues-detected');
      expect(result.data.issues).toContain('continuum-sidebar: Shadow DOM is empty');
    });
  });

  describe('batch()', () => {
    test('should execute multiple probes', () => {
      const results = jtag.batch(['widgets', 'customElements']);
      
      expect(results).toHaveProperty('widgets');
      expect(results).toHaveProperty('customElements');
      expect(results.widgets.success).toBe(true);
      expect(results.customElements.success).toBe(true);
    });
  });

  describe('throttling', () => {
    test('should throttle rapid probe calls', () => {
      const options = { throttle: 1000 };
      
      const result1 = jtag.widgets(options);
      const result2 = jtag.widgets(options);
      
      expect(result1.success).toBe(true);
      expect(result2.success).toBe(false);
      expect(result2.data.throttled).toBe(true);
    });
  });

  describe('auto-logging', () => {
    test('should auto-log probes by default', () => {
      jtag.widgets();
      
      expect(mockConsoleProbe).toHaveBeenCalledWith(
        expect.objectContaining({
          message: expect.stringContaining('JTAG Probe: widgets'),
          category: 'widgets'
        })
      );
    });

    test('should respect autoLog: false option', () => {
      jtag.widgets({ autoLog: false });
      
      expect(mockConsoleProbe).not.toHaveBeenCalled();
    });
  });
});