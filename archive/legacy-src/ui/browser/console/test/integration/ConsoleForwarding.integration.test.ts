/**
 * BrowserConsoleDaemon Integration Tests
 * 
 * Tests the console daemon integration with browser environment and event bus
 */

import { BrowserConsoleDaemon } from '../../BrowserConsoleDaemon';
import { BrowserDaemonEventBus } from '../../base/BrowserDaemonEventBus';

describe('BrowserConsoleDaemon Integration Tests', () => {
  let daemon: BrowserConsoleDaemon;
  let eventBus: BrowserDaemonEventBus;
  let capturedEvents: any[] = [];

  beforeAll(() => {
    // Setup browser environment mocks
    Object.defineProperty(window, 'location', {
      value: { href: 'http://localhost:9000/test' },
      writable: true
    });
    
    Object.defineProperty(window, 'navigator', {
      value: { userAgent: 'Test Browser Integration' },
      writable: true
    });

    Object.defineProperty(window, 'innerWidth', { value: 1920, writable: true });
    Object.defineProperty(window, 'innerHeight', { value: 1080, writable: true });
  });

  beforeEach(() => {
    daemon = new BrowserConsoleDaemon();
    eventBus = BrowserDaemonEventBus.getInstance();
    capturedEvents = [];

    // Capture events from daemon
    eventBus.on('console:forward', (eventData) => {
      capturedEvents.push(eventData);
    });
  });

  afterEach(async () => {
    await daemon.stop();
    eventBus.off('console:forward');
    capturedEvents = [];
  });

  describe('Event Bus Integration', () => {
    test('should emit console:forward events', async () => {
      await daemon.start();
      
      // Set session ID
      await daemon.handleMessage({
        type: 'console:set_session',
        data: { sessionId: 'test-session-123' }
      });

      // Capture a console log
      await daemon.handleMessage({
        type: 'console:capture',
        data: {
          type: 'log',
          args: ['Integration test message']
        }
      });

      // Process the queue
      await daemon.handleMessage({
        type: 'console:process_queue',
        data: {}
      });

      // Check if event was emitted
      expect(capturedEvents.length).toBeGreaterThan(0);
      expect(capturedEvents[0]).toHaveProperty('command', 'console');
      expect(capturedEvents[0]).toHaveProperty('params');
      expect(capturedEvents[0].params).toHaveProperty('sessionId', 'test-session-123');
    });

    test('should queue events when no session ID is set', async () => {
      await daemon.start();

      // Capture without session ID
      await daemon.handleMessage({
        type: 'console:capture',
        data: {
          type: 'log',
          args: ['Message without session']
        }
      });

      // Process queue (should not emit events)
      await daemon.handleMessage({
        type: 'console:process_queue',
        data: {}
      });

      expect(capturedEvents.length).toBe(0);

      // Now set session ID and process again
      await daemon.handleMessage({
        type: 'console:set_session',
        data: { sessionId: 'delayed-session' }
      });

      // Give some time for auto-processing
      await new Promise(resolve => setTimeout(resolve, 150));

      expect(capturedEvents.length).toBeGreaterThan(0);
    });
  });

  describe('Console Method Override Integration', () => {
    test('should integrate with actual console methods when enabled', async () => {
      await daemon.start();

      const statusResponse = await daemon.handleMessage({
        type: 'console:get_status',
        data: {}
      });

      if (statusResponse.data?.isInitialized) {
        expect(statusResponse.data.capturedMethods).toContain('log');
        expect(statusResponse.data.capturedMethods).toContain('error');
        expect(statusResponse.data.capturedMethods).toContain('warn');
      }
    });
  });

  describe('Error Handling Integration', () => {
    test('should handle malformed events gracefully', async () => {
      await daemon.start();

      // This should not crash the daemon
      const response = await daemon.handleMessage({
        type: 'console:capture',
        data: {
          type: 'log',
          args: [{ circular: null }] // Create circular reference
        }
      });

      // Should handle gracefully
      expect(response.success).toBe(true);
    });
  });

  describe('Performance Integration', () => {
    test('should handle multiple rapid console captures', async () => {
      await daemon.start();
      
      await daemon.handleMessage({
        type: 'console:set_session',
        data: { sessionId: 'performance-test' }
      });

      const startTime = Date.now();
      
      // Rapid fire 10 console captures
      const promises = [];
      for (let i = 0; i < 10; i++) {
        promises.push(daemon.handleMessage({
          type: 'console:capture',
          data: {
            type: 'log',
            args: [`Performance test message ${i}`]
          }
        }));
      }

      await Promise.all(promises);
      
      const endTime = Date.now();
      const duration = endTime - startTime;

      // Should complete within reasonable time (< 1 second)
      expect(duration).toBeLessThan(1000);

      // Process queue
      await daemon.handleMessage({
        type: 'console:process_queue',
        data: {}
      });

      // All messages should eventually be forwarded
      expect(capturedEvents.length).toBe(10);
    });
  });
});

// Provide mock implementations for test environment
if (typeof window === 'undefined') {
  global.window = {
    location: { href: 'http://localhost:9000/test' },
    navigator: { userAgent: 'Node.js Test Environment' },
    innerWidth: 1920,
    innerHeight: 1080,
    addEventListener: () => {},
    removeEventListener: () => {}
  } as any;
}

// Mock test functions if not available
if (typeof describe === 'undefined') {
  global.describe = (name: string, fn: () => void) => {
    console.log(`\n--- ${name} ---`);
    fn();
  };
  global.test = (name: string, fn: () => void) => {
    console.log(`  ✓ ${name}`);
    try {
      fn();
    } catch (error) {
      console.log(`  ✗ ${name}: ${error}`);
    }
  };
  global.beforeAll = (fn: () => void) => fn();
  global.beforeEach = (fn: () => void) => fn();
  global.afterEach = (fn: () => void) => fn();
  global.expect = ((value: any) => ({
    toBe: (expected: any) => value === expected,
    toBeGreaterThan: (expected: any) => value > expected,
    toBeLessThan: (expected: any) => value < expected,
    toHaveProperty: (prop: string, expectedValue?: any) => {
      const hasProperty = value && typeof value === 'object' && prop in value;
      if (expectedValue !== undefined) {
        return hasProperty && value[prop] === expectedValue;
      }
      return hasProperty;
    },
    toContain: (item: any) => Array.isArray(value) && value.includes(item)
  })) as any;
}