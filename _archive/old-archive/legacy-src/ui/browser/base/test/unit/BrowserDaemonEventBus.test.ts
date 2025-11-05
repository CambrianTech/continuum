/**
 * BrowserDaemonEventBus Unit Tests
 * 
 * Tests the browser-side event bus following the same patterns
 * as server-side event bus testing.
 */

import { test } from 'node:test';
import assert from 'node:assert';
import { BrowserDaemonEventBus, BrowserDaemonEvent, BrowserEventHandler } from '../../BrowserDaemonEventBus';

test('BrowserDaemonEventBus Singleton', async (t) => {
  await t.test('should return same instance', () => {
    const eventBus1 = BrowserDaemonEventBus.getInstance();
    const eventBus2 = BrowserDaemonEventBus.getInstance();

    assert.strictEqual(eventBus1, eventBus2);
  });

  await t.test('should clear properly', () => {
    const eventBus = BrowserDaemonEventBus.getInstance();
    
    // Add some handlers
    eventBus.on('test.event', () => {});
    assert.strictEqual(eventBus.getHandlerCount('test.event'), 1);

    // Clear
    eventBus.clear();
    assert.strictEqual(eventBus.getHandlerCount('test.event'), 0);
    assert.deepStrictEqual(eventBus.getRegisteredEventTypes(), []);
  });
});

test('BrowserDaemonEventBus Event Subscription', async (t) => {
  await t.test('should register event handlers', () => {
    const eventBus = BrowserDaemonEventBus.getInstance();
    eventBus.clear();

    const handler = () => {};
    eventBus.on('test.event', handler);

    assert.strictEqual(eventBus.getHandlerCount('test.event'), 1);
    assert.deepStrictEqual(eventBus.getRegisteredEventTypes(), ['test.event']);
  });

  await t.test('should register multiple handlers for same event', () => {
    const eventBus = BrowserDaemonEventBus.getInstance();
    eventBus.clear();

    const handler1 = () => {};
    const handler2 = () => {};
    
    eventBus.on('test.event', handler1);
    eventBus.on('test.event', handler2);

    assert.strictEqual(eventBus.getHandlerCount('test.event'), 2);
  });

  await t.test('should unregister specific handler', () => {
    const eventBus = BrowserDaemonEventBus.getInstance();
    eventBus.clear();

    const handler1 = () => {};
    const handler2 = () => {};
    
    eventBus.on('test.event', handler1);
    eventBus.on('test.event', handler2);
    assert.strictEqual(eventBus.getHandlerCount('test.event'), 2);

    eventBus.off('test.event', handler1);
    assert.strictEqual(eventBus.getHandlerCount('test.event'), 1);
  });

  await t.test('should unregister all handlers for event type', () => {
    const eventBus = BrowserDaemonEventBus.getInstance();
    eventBus.clear();

    eventBus.on('test.event', () => {});
    eventBus.on('test.event', () => {});
    assert.strictEqual(eventBus.getHandlerCount('test.event'), 2);

    eventBus.off('test.event');
    assert.strictEqual(eventBus.getHandlerCount('test.event'), 0);
  });
});

test('BrowserDaemonEventBus Event Emission', async (t) => {
  await t.test('should emit events to handlers', async () => {
    const eventBus = BrowserDaemonEventBus.getInstance();
    eventBus.clear();

    let receivedEvent: BrowserDaemonEvent | null = null;
    const handler: BrowserEventHandler = (event) => {
      receivedEvent = event;
    };

    eventBus.on('test.event', handler);
    await eventBus.emit('test.event', { test: 'data' }, 'test-source');

    assert.ok(receivedEvent);
    assert.strictEqual(receivedEvent.type, 'test.event');
    assert.deepStrictEqual(receivedEvent.data, { test: 'data' });
    assert.strictEqual(receivedEvent.source, 'test-source');
    assert.ok(receivedEvent.timestamp);
    assert.ok(receivedEvent.id);
  });

  await t.test('should emit to multiple handlers', async () => {
    const eventBus = BrowserDaemonEventBus.getInstance();
    eventBus.clear();

    const receivedEvents: BrowserDaemonEvent[] = [];
    const handler1: BrowserEventHandler = (event) => receivedEvents.push(event);
    const handler2: BrowserEventHandler = (event) => receivedEvents.push(event);

    eventBus.on('test.event', handler1);
    eventBus.on('test.event', handler2);
    await eventBus.emit('test.event', { test: 'data' });

    assert.strictEqual(receivedEvents.length, 2);
    assert.strictEqual(receivedEvents[0].type, 'test.event');
    assert.strictEqual(receivedEvents[1].type, 'test.event');
  });

  await t.test('should handle async handlers', async () => {
    const eventBus = BrowserDaemonEventBus.getInstance();
    eventBus.clear();

    let handlerComplete = false;
    const asyncHandler: BrowserEventHandler = async (event) => {
      await new Promise(resolve => setTimeout(resolve, 10));
      handlerComplete = true;
    };

    eventBus.on('test.event', asyncHandler);
    await eventBus.emit('test.event', {});

    assert.strictEqual(handlerComplete, true);
  });

  await t.test('should handle handler errors gracefully', async () => {
    const eventBus = BrowserDaemonEventBus.getInstance();
    eventBus.clear();

    const errorHandler: BrowserEventHandler = () => {
      throw new Error('Handler error');
    };

    const successHandler: BrowserEventHandler = () => {
      // Should still execute despite error in other handler
    };

    eventBus.on('test.event', errorHandler);
    eventBus.on('test.event', successHandler);

    // Should not throw despite handler error
    await assert.doesNotReject(async () => {
      await eventBus.emit('test.event', {});
    });
  });
});

test('BrowserDaemonEventBus Event History', async (t) => {
  await t.test('should maintain event history', async () => {
    const eventBus = BrowserDaemonEventBus.getInstance();
    eventBus.clear();

    await eventBus.emit('test.event1', { data: 1 });
    await eventBus.emit('test.event2', { data: 2 });
    await eventBus.emit('test.event3', { data: 3 });

    const history = eventBus.getEventHistory();
    assert.strictEqual(history.length, 3);
    assert.strictEqual(history[0].type, 'test.event1');
    assert.strictEqual(history[1].type, 'test.event2');
    assert.strictEqual(history[2].type, 'test.event3');
  });

  await t.test('should limit event history size', async () => {
    const eventBus = BrowserDaemonEventBus.getInstance();
    eventBus.clear();

    const history = eventBus.getEventHistory(2);
    assert.ok(history.length <= 2);
  });
});

test('BrowserDaemonEventBus Diagnostics', async (t) => {
  await t.test('should provide diagnostics information', () => {
    const eventBus = BrowserDaemonEventBus.getInstance();
    eventBus.clear();

    eventBus.on('test.event1', () => {});
    eventBus.on('test.event1', () => {});
    eventBus.on('test.event2', () => {});

    const diagnostics = eventBus.getDiagnostics();

    assert.deepStrictEqual(diagnostics.registeredEventTypes, ['test.event1', 'test.event2']);
    assert.strictEqual(diagnostics.totalHandlers, 3);
    assert.strictEqual(diagnostics.eventHistorySize, 0); // No events emitted yet
    assert.ok(Array.isArray(diagnostics.recentEvents));
  });
});