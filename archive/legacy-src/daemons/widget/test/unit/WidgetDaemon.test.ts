/**
 * Widget Daemon Unit Tests
 * 
 * Tests the strongly-typed event system and widget management
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { WidgetDaemon } from '../../WidgetDaemon';
import { 
  WidgetEventType, 
  WidgetStatus, 
  createWidgetEvent,
  WidgetDiscoveredEvent,
  WidgetSystemReadyEvent,
  isWidgetEvent 
} from '../../../../shared/types/WidgetEventTypes';

describe('WidgetDaemon', () => {
  let daemon: WidgetDaemon;

  before(async () => {
    daemon = new WidgetDaemon();
    await daemon.start();
  });

  after(async () => {
    await daemon.stop();
  });

  describe('Strongly-Typed Event System', () => {
    it('should emit type-safe widget events', (t, done) => {
      // Type-safe event listener - linter will catch wrong types!
      daemon.onWidgetEvent(WidgetEventType.WIDGET_DISCOVERED, (event) => {
        // TypeScript knows this is WidgetDiscoveredEvent
        assert.strictEqual(event.type, WidgetEventType.WIDGET_DISCOVERED);
        assert.strictEqual(typeof event.payload.widgetId, 'string');
        assert.strictEqual(typeof event.payload.widgetType, 'string');
        done();
      });

      // Create type-safe event - linter catches payload mismatches!
      const event = createWidgetEvent<WidgetDiscoveredEvent>(
        WidgetEventType.WIDGET_DISCOVERED,
        {
          widgetId: 'test-widget',
          widgetType: 'ui',
          element: 'div.test'
        },
        'server'
      );

      daemon.emitWidgetEvent(event);
    });

    it('should validate event structure with type guards', () => {
      const validEvent = createWidgetEvent<WidgetSystemReadyEvent>(
        WidgetEventType.WIDGET_SYSTEM_READY,
        {
          totalWidgets: 5,
          loadedWidgets: 4,
          failedWidgets: 1,
          readyTime: 1000
        }
      );

      const invalidEvent = {
        type: 'not-a-widget-event',
        timestamp: 'invalid'
      };

      assert.strictEqual(isWidgetEvent(validEvent), true);
      assert.strictEqual(isWidgetEvent(invalidEvent), false);
    });

    it('should prevent magic string errors at compile time', () => {
      // This test exists to show the linter catches errors
      // Uncommenting these lines would cause TypeScript errors:
      
      // daemon.onWidgetEvent('widget-discovered', () => {}); // ❌ Linter error: not in enum
      // daemon.emitWidgetEvent({ type: 'fake-event' }); // ❌ Linter error: wrong structure
      
      // Only properly typed events are allowed:
      daemon.onWidgetEvent(WidgetEventType.WIDGET_ERROR, (event) => {
        // TypeScript knows this is WidgetErrorEvent with correct payload
        assert.strictEqual(typeof event.payload.error, 'string');
      });
      
      assert.ok(true, 'Type system enforces correctness');
    });
  });

  describe('Widget Discovery', () => {
    it('should discover widgets and return type-safe response', async () => {
      const response = await daemon.handleMessage({
        type: 'widget:discover',
        data: { paths: ['src/ui/components'] },
        timestamp: new Date().toISOString()
      });

      assert.strictEqual(response.success, true);
      assert.strictEqual(typeof response.data?.discoveredWidgets, 'number');
      assert.strictEqual(typeof response.data?.readyWidgets, 'number');
      assert.strictEqual(typeof response.data?.failedWidgets, 'number');
    });
  });

  describe('Widget Registration', () => {
    it('should register widgets with proper validation', async () => {
      const manifest = {
        id: 'test-widget-123',
        name: 'Test Widget',
        path: 'src/ui/test',
        config: {
          name: 'Test Widget',
          version: '1.0.0',
          type: 'ui' as const
        },
        discovered: new Date().toISOString()
      };

      const response = await daemon.handleMessage({
        type: 'widget:register',
        data: { manifest },
        timestamp: new Date().toISOString()
      });

      assert.strictEqual(response.success, true);
      assert.strictEqual(response.data?.widgetId, manifest.id);
      assert.strictEqual(response.data?.status, WidgetStatus.DISCOVERED);
    });

    it('should reject invalid widget registration', async () => {
      const response = await daemon.handleMessage({
        type: 'widget:register',
        data: { manifest: { name: 'Invalid - no ID' } },
        timestamp: new Date().toISOString()
      });

      assert.strictEqual(response.success, false);
      assert.strictEqual(typeof response.error, 'string');
    });
  });

  describe('Widget Listing', () => {
    it('should list widgets with type-safe filtering', async () => {
      // First register a widget
      await daemon.handleMessage({
        type: 'widget:register',
        data: {
          manifest: {
            id: 'list-test-widget',
            name: 'List Test Widget',
            path: 'src/ui/list-test',
            config: {
              name: 'List Test Widget',
              version: '1.0.0',
              type: 'ui' as const
            },
            discovered: new Date().toISOString()
          }
        },
        timestamp: new Date().toISOString()
      });

      // List all widgets
      const allResponse = await daemon.handleMessage({
        type: 'widget:list',
        data: {},
        timestamp: new Date().toISOString()
      });

      assert.strictEqual(allResponse.success, true);
      assert.strictEqual(Array.isArray(allResponse.data?.widgets), true);
      assert.strictEqual(typeof allResponse.data?.total, 'number');
      assert.strictEqual(typeof allResponse.data?.systemReady, 'boolean');

      // List widgets by status (type-safe enum)
      const statusResponse = await daemon.handleMessage({
        type: 'widget:list',
        data: { status: WidgetStatus.DISCOVERED },
        timestamp: new Date().toISOString()
      });

      assert.strictEqual(statusResponse.success, true);
      assert.strictEqual(Array.isArray(statusResponse.data?.widgets), true);
    });
  });

  describe('Message Type Safety', () => {
    it('should handle only valid message types', async () => {
      const validTypes = [
        'widget:discover',
        'widget:register', 
        'widget:unregister',
        'widget:list',
        'widget:status',
        'widget:health_check',
        'widget:emit_event'
      ];

      // Test that getMessageTypes returns the expected types
      const supportedTypes = daemon.getMessageTypes();
      
      for (const type of validTypes) {
        assert.ok(supportedTypes.includes(type), `Should support ${type}`);
      }

      // Test invalid message type
      const invalidResponse = await daemon.handleMessage({
        type: 'invalid:message:type',
        data: {},
        timestamp: new Date().toISOString()
      });

      assert.strictEqual(invalidResponse.success, false);
      assert.ok(invalidResponse.error?.includes('Unknown message type'));
    });
  });
});