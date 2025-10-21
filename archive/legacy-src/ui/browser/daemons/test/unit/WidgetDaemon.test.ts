/**
 * WidgetDaemon Unit Tests
 * 
 * Tests the extracted widget discovery, health validation, and event management functionality
 */

import { describe, it, test, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert';
import { WidgetDaemon } from '../../WidgetDaemon';

// Mock DOM environment for Node.js testing
function setupMockDOM() {
  const mockDocument = {
    querySelectorAll: (selector: string) => {
      // Mock some existing widgets in DOM
      if (selector === 'continuum-sidebar, chat-widget') {
        return [
          { tagName: 'CONTINUUM-SIDEBAR' },
          { tagName: 'CHAT-WIDGET' }
        ];
      }
      if (selector === 'chat-widget') {
        return [{ tagName: 'CHAT-WIDGET' }];
      }
      if (selector === 'continuum-sidebar') {
        return [{ tagName: 'CONTINUUM-SIDEBAR' }];
      }
      return [];
    },
    querySelector: (selector: string) => {
      // Mock individual widget queries
      if (selector === 'chat-widget') {
        return { tagName: 'CHAT-WIDGET' };
      }
      if (selector === 'continuum-sidebar') {
        return { tagName: 'CONTINUUM-SIDEBAR' };
      }
      return null;
    },
    body: {}
  };

  const mockWindow = {
    getComputedStyle: () => ({
      display: 'block',
      visibility: 'visible',
      cssText: 'display: block; width: 300px; height: 400px;',
      length: 25
    }),
    MutationObserver: undefined, // Disable for testing
    Node: {
      ELEMENT_NODE: 1
    }
  };

  // Setup global mocks
  global.document = mockDocument as any;
  global.window = mockWindow as any;
  global.MutationObserver = undefined;
  global.Node = mockWindow.Node as any;

  return { mockDocument, mockWindow };
}

describe('WidgetDaemon Unit Tests', () => {
  let daemon: WidgetDaemon;
  let mockDOM: any;

  beforeEach(() => {
    mockDOM = setupMockDOM();
    daemon = new WidgetDaemon();
  });

  afterEach(() => {
    daemon.destroy();
  });

  describe('Initialization', () => {
    test('should instantiate correctly', () => {
      assert.ok(daemon instanceof WidgetDaemon, 'Should create WidgetDaemon instance');
    });

    test('should initialize successfully', async () => {
      await assert.doesNotReject(async () => {
        await daemon.initialize();
      }, 'Initialization should not throw');
    });

    test('should have default known widgets', () => {
      const knownWidgets = daemon.getKnownWidgets();
      assert.ok(knownWidgets.includes('chat-widget'), 'Should include chat-widget');
      assert.ok(knownWidgets.includes('continuum-sidebar'), 'Should include continuum-sidebar');
    });
  });

  describe('Widget Discovery', () => {
    test('should discover widgets in DOM', async () => {
      const result = await daemon.discoverAndLoadWidgets();
      
      assert.strictEqual(result.totalWidgets, 2, 'Should find 2 widgets');
      assert.ok(result.loadedWidgets.includes('continuum-sidebar'), 'Should include sidebar');
      assert.ok(result.loadedWidgets.includes('chat-widget'), 'Should include chat widget');
      assert.strictEqual(result.discoveryMethod, 'html-dom', 'Should use HTML DOM discovery');
    });

    test('should mark widget loading as complete after discovery', async () => {
      assert.strictEqual(daemon.isWidgetLoadingComplete(), false, 'Should start as incomplete');
      
      await daemon.discoverAndLoadWidgets();
      
      assert.strictEqual(daemon.isWidgetLoadingComplete(), true, 'Should be complete after discovery');
    });

    test('should return proper discovery result structure', async () => {
      const result = await daemon.discoverAndLoadWidgets();
      
      assert.ok(typeof result.totalWidgets === 'number', 'totalWidgets should be number');
      assert.ok(Array.isArray(result.loadedWidgets), 'loadedWidgets should be array');
      assert.ok(Array.isArray(result.customElements), 'customElements should be array');
      assert.ok(typeof result.discoveryMethod === 'string', 'discoveryMethod should be string');
    });
  });

  describe('Widget Health Validation', () => {
    test('should validate widget health', async () => {
      const healthComponents = await daemon.validateWidgetHealth();
      
      assert.strictEqual(healthComponents.length, 2, 'Should check 2 widgets');
      
      // Check first widget (chat-widget)
      const chatWidget = healthComponents.find(c => c.component === 'chat-widget');
      assert.ok(chatWidget, 'Should include chat-widget health');
      assert.strictEqual(chatWidget.status, 'healthy', 'Chat widget should be healthy');
      assert.ok(chatWidget.metrics.hasElement, 'Should detect widget element');
      assert.ok(chatWidget.metrics.isVisible, 'Should detect widget visibility');
      assert.ok(chatWidget.metrics.hasStyles, 'Should detect widget styles');
    });

    test('should handle missing widgets correctly', async () => {
      // Mock no widgets found
      mockDOM.mockDocument.querySelector = () => null;
      
      const healthComponents = await daemon.validateWidgetHealth();
      
      healthComponents.forEach(component => {
        assert.strictEqual(component.status, 'failed', 'Missing widgets should be failed');
        assert.strictEqual(component.metrics.hasElement, false, 'Should detect missing element');
        assert.ok(component.details.includes('missing from DOM'), 'Should indicate missing from DOM');
      });
    });

    test('should detect degraded widget status', async () => {
      // Mock widget with no styles
      mockDOM.mockWindow.getComputedStyle = () => ({
        display: 'block',
        visibility: 'visible',
        cssText: '', // No styles
        length: 0
      });
      
      const healthComponents = await daemon.validateWidgetHealth();
      
      const degradedWidget = healthComponents.find(c => c.status === 'degraded');
      assert.ok(degradedWidget, 'Should detect degraded widget');
      assert.ok(degradedWidget.details.includes('no styles'), 'Should indicate missing styles');
    });
  });

  describe('Widget Registration', () => {
    test('should register new widget types', () => {
      const initialCount = daemon.getKnownWidgets().length;
      
      daemon.registerWidget('new-widget');
      
      assert.strictEqual(daemon.getKnownWidgets().length, initialCount + 1, 'Should add new widget');
      assert.ok(daemon.getKnownWidgets().includes('new-widget'), 'Should include new widget');
    });

    test('should not register duplicate widgets', () => {
      const initialCount = daemon.getKnownWidgets().length;
      
      daemon.registerWidget('chat-widget'); // Already exists
      
      assert.strictEqual(daemon.getKnownWidgets().length, initialCount, 'Should not add duplicate');
    });

    test('should unregister widget types', () => {
      daemon.registerWidget('temp-widget');
      assert.ok(daemon.getKnownWidgets().includes('temp-widget'), 'Widget should be registered');
      
      daemon.unregisterWidget('temp-widget');
      assert.ok(!daemon.getKnownWidgets().includes('temp-widget'), 'Widget should be unregistered');
    });

    test('should handle unregistering non-existent widgets', () => {
      const initialCount = daemon.getKnownWidgets().length;
      
      daemon.unregisterWidget('non-existent-widget');
      
      assert.strictEqual(daemon.getKnownWidgets().length, initialCount, 'Count should remain same');
    });
  });

  describe('Event System', () => {
    test('should add and remove event listeners', () => {
      let eventFired = false;
      const handler = () => { eventFired = true; };
      
      daemon.on('test-event', handler);
      daemon.emit('test-event');
      
      assert.strictEqual(eventFired, true, 'Event handler should be called');
      
      eventFired = false;
      daemon.off('test-event', handler);
      daemon.emit('test-event');
      
      assert.strictEqual(eventFired, false, 'Event handler should be removed');
    });

    test('should handle multiple event listeners', () => {
      let count = 0;
      const handler1 = () => { count++; };
      const handler2 = () => { count++; };
      
      daemon.on('test-event', handler1);
      daemon.on('test-event', handler2);
      daemon.emit('test-event');
      
      assert.strictEqual(count, 2, 'Both handlers should be called');
    });

    test('should remove all handlers when no specific handler provided', () => {
      let count = 0;
      daemon.on('test-event', () => { count++; });
      daemon.on('test-event', () => { count++; });
      
      daemon.off('test-event'); // Remove all
      daemon.emit('test-event');
      
      assert.strictEqual(count, 0, 'All handlers should be removed');
    });

    test('should handle continuum ready event', async () => {
      let readyEventFired = false;
      daemon.on('widgets:ready', () => { readyEventFired = true; });
      
      await daemon.handleContinuumReady();
      
      assert.strictEqual(readyEventFired, true, 'widgets:ready event should be fired');
      assert.strictEqual(daemon.isWidgetLoadingComplete(), true, 'Widget loading should be complete');
    });
  });

  describe('Status and Information', () => {
    test('should return comprehensive widget status', async () => {
      await daemon.discoverAndLoadWidgets(); // Ensure widgets are loaded
      
      const status = await daemon.getWidgetStatus();
      
      assert.ok(typeof status.widgetLoadingComplete === 'boolean', 'Should include loading status');
      assert.ok(typeof status.totalWidgets === 'number', 'Should include total widget count');
      assert.ok(Array.isArray(status.loadedWidgets), 'Should include loaded widgets list');
      assert.ok(Array.isArray(status.healthComponents), 'Should include health components');
      assert.ok(Array.isArray(status.knownWidgets), 'Should include known widgets list');
      assert.ok(status.status, 'Should include status summary');
      assert.ok(typeof status.status.healthy === 'number', 'Should include healthy count');
      assert.ok(typeof status.status.degraded === 'number', 'Should include degraded count');
      assert.ok(typeof status.status.failed === 'number', 'Should include failed count');
    });

    test('should return known widgets list', () => {
      const knownWidgets = daemon.getKnownWidgets();
      
      assert.ok(Array.isArray(knownWidgets), 'Should return array');
      assert.ok(knownWidgets.length > 0, 'Should have known widgets');
    });
  });

  describe('Error Handling', () => {
    test('should handle event handler errors gracefully', () => {
      const errorHandler = () => { throw new Error('Test error'); };
      const normalHandler = () => {}; // Should still execute
      
      daemon.on('test-event', errorHandler);
      daemon.on('test-event', normalHandler);
      
      assert.doesNotThrow(() => {
        daemon.emit('test-event');
      }, 'Should not throw when handler throws');
    });

    test('should handle DOM query errors gracefully', async () => {
      // Mock querySelector to throw
      mockDOM.mockDocument.querySelector = () => { throw new Error('DOM error'); };
      
      await assert.doesNotReject(async () => {
        await daemon.validateWidgetHealth();
      }, 'Should handle DOM errors gracefully');
    });
  });

  describe('Cleanup', () => {
    test('should clean up resources on destroy', () => {
      // Add some state
      daemon.on('test-event', () => {});
      daemon.registerWidget('temp-widget');
      
      daemon.destroy();
      
      assert.strictEqual(daemon.isWidgetLoadingComplete(), false, 'Should reset loading status');
      
      // Event handlers should be cleared (can't directly test Map.clear(), but ensure no crash)
      assert.doesNotThrow(() => {
        daemon.emit('test-event');
      }, 'Should handle events after destroy gracefully');
    });
  });
});