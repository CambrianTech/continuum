/**
 * Browser Daemon System Integration Tests
 * 
 * Tests the complete browser daemon system integration including
 * manager, event bus, and daemon coordination.
 */

import { test } from 'node:test';
import assert from 'node:assert';
import { BaseBrowserDaemon, BrowserDaemonMessage, BrowserDaemonResponse } from '../../BaseBrowserDaemon';
import { BrowserDaemonManager } from '../../BrowserDaemonManager';
import { BrowserDaemonEventBus } from '../../BrowserDaemonEventBus';

// Test daemon implementations
class TestConsoleDaemon extends BaseBrowserDaemon {
  constructor() {
    super('console-daemon', '1.0.0');
  }

  getMessageTypes(): string[] {
    return ['console.capture', 'console.forward'];
  }

  async handleMessage(message: BrowserDaemonMessage): Promise<BrowserDaemonResponse> {
    switch (message.type) {
      case 'console.capture':
        return this.createSuccessResponse({ captured: true, data: message.data });
      case 'console.forward':
        return this.createSuccessResponse({ forwarded: true, target: 'server' });
      default:
        return this.createErrorResponse(`Unsupported message type: ${message.type}`);
    }
  }
}

class TestWebSocketDaemon extends BaseBrowserDaemon {
  constructor() {
    super('websocket-daemon', '1.0.0');
  }

  getMessageTypes(): string[] {
    return ['websocket.connect', 'websocket.send'];
  }

  async handleMessage(message: BrowserDaemonMessage): Promise<BrowserDaemonResponse> {
    switch (message.type) {
      case 'websocket.connect':
        return this.createSuccessResponse({ connected: true, url: message.data.url });
      case 'websocket.send':
        return this.createSuccessResponse({ sent: true, messageId: 'msg_123' });
      default:
        return this.createErrorResponse(`Unsupported message type: ${message.type}`);
    }
  }
}

test('Browser Daemon System Integration', async (t) => {
  await t.test('should initialize manager and register daemons', async () => {
    const manager = new BrowserDaemonManager();
    await manager.initialize();

    const consoleDaemon = new TestConsoleDaemon();
    const webSocketDaemon = new TestWebSocketDaemon();

    await manager.registerDaemon('console', consoleDaemon);
    await manager.registerDaemon('websocket', webSocketDaemon);

    const registeredDaemons = manager.getRegisteredDaemons();
    assert.strictEqual(registeredDaemons.length, 2);

    const consoleReg = manager.getDaemon('console');
    const webSocketReg = manager.getDaemon('websocket');

    assert.ok(consoleReg);
    assert.ok(webSocketReg);
    assert.deepStrictEqual(consoleReg.messageTypes, ['console.capture', 'console.forward']);
    assert.deepStrictEqual(webSocketReg.messageTypes, ['websocket.connect', 'websocket.send']);
  });

  await t.test('should start and stop daemons', async () => {
    const manager = new BrowserDaemonManager();
    await manager.initialize();

    const consoleDaemon = new TestConsoleDaemon();
    await manager.registerDaemon('console', consoleDaemon);

    // Initially not running
    let consoleReg = manager.getDaemon('console')!;
    assert.strictEqual(consoleReg.isRunning, false);

    // Start daemon
    await manager.startDaemon('console');
    consoleReg = manager.getDaemon('console')!;
    assert.strictEqual(consoleReg.isRunning, true);

    // Stop daemon
    await manager.stopDaemon('console');
    consoleReg = manager.getDaemon('console')!;
    assert.strictEqual(consoleReg.isRunning, false);
  });

  await t.test('should route messages to correct daemon', async () => {
    const manager = new BrowserDaemonManager();
    await manager.initialize();

    const consoleDaemon = new TestConsoleDaemon();
    const webSocketDaemon = new TestWebSocketDaemon();

    await manager.registerDaemon('console', consoleDaemon);
    await manager.registerDaemon('websocket', webSocketDaemon);
    await manager.startDaemon('console');
    await manager.startDaemon('websocket');

    // Test console message routing
    const consoleMessage: BrowserDaemonMessage = {
      type: 'console.capture',
      data: { log: 'test message' },
      timestamp: new Date().toISOString()
    };

    const consoleResponse = await manager.routeMessage(consoleMessage);
    assert.strictEqual(consoleResponse.success, true);
    assert.strictEqual(consoleResponse.data.captured, true);

    // Test websocket message routing
    const webSocketMessage: BrowserDaemonMessage = {
      type: 'websocket.connect',
      data: { url: 'ws://localhost:9000' },
      timestamp: new Date().toISOString()
    };

    const webSocketResponse = await manager.routeMessage(webSocketMessage);
    assert.strictEqual(webSocketResponse.success, true);
    assert.strictEqual(webSocketResponse.data.connected, true);
  });

  await t.test('should handle unknown message types', async () => {
    const manager = new BrowserDaemonManager();
    await manager.initialize();

    const unknownMessage: BrowserDaemonMessage = {
      type: 'unknown.message',
      data: {},
      timestamp: new Date().toISOString()
    };

    const response = await manager.routeMessage(unknownMessage);
    assert.strictEqual(response.success, false);
    assert.ok(response.error?.includes('No daemon registered'));
  });

  await t.test('should provide system diagnostics', async () => {
    const manager = new BrowserDaemonManager();
    await manager.initialize();

    const consoleDaemon = new TestConsoleDaemon();
    await manager.registerDaemon('console', consoleDaemon);
    await manager.startDaemon('console');

    const diagnostics = manager.getDiagnostics();

    assert.strictEqual(diagnostics.isInitialized, true);
    assert.strictEqual(diagnostics.totalDaemons, 1);
    assert.strictEqual(diagnostics.runningDaemons, 1);
    assert.strictEqual(diagnostics.stoppedDaemons, 0);

    assert.ok(diagnostics.daemons);
    assert.strictEqual(diagnostics.daemons[0].name, 'console');
    assert.strictEqual(diagnostics.daemons[0].isRunning, true);

    assert.ok(diagnostics.eventBus);
    assert.ok(diagnostics.featureFlags);
  });
});

test('Browser Daemon Event Communication', async (t) => {
  await t.test('should coordinate daemons via event bus', async () => {
    const eventBus = BrowserDaemonEventBus.getInstance();
    eventBus.clear();

    const manager = new BrowserDaemonManager();
    await manager.initialize();

    const consoleDaemon = new TestConsoleDaemon();
    await manager.registerDaemon('console', consoleDaemon);
    await manager.startDaemon('console');

    // Listen for daemon events
    const events: any[] = [];
    eventBus.on('daemon:started', (event) => {
      events.push(event);
    });

    // Start another daemon to trigger event
    const webSocketDaemon = new TestWebSocketDaemon();
    await manager.registerDaemon('websocket', webSocketDaemon);
    await manager.startDaemon('websocket');

    // Should have received start event
    assert.strictEqual(events.length, 1);
    assert.strictEqual(events[0].data.name, 'websocket');
  });
});

test('Browser Daemon Error Handling', async (t) => {
  await t.test('should handle daemon registration errors', async () => {
    const manager = new BrowserDaemonManager();
    await manager.initialize();

    const daemon1 = new TestConsoleDaemon();
    const daemon2 = new TestConsoleDaemon();

    await manager.registerDaemon('console', daemon1);

    // Should throw on duplicate registration
    await assert.rejects(async () => {
      await manager.registerDaemon('console', daemon2);
    }, /already registered/);
  });

  await t.test('should handle start/stop of non-existent daemon', async () => {
    const manager = new BrowserDaemonManager();
    await manager.initialize();

    await assert.rejects(async () => {
      await manager.startDaemon('non-existent');
    }, /not registered/);

    await assert.rejects(async () => {
      await manager.stopDaemon('non-existent');
    }, /not registered/);
  });

  await t.test('should handle emergency shutdown', async () => {
    const manager = new BrowserDaemonManager();
    await manager.initialize();

    const consoleDaemon = new TestConsoleDaemon();
    const webSocketDaemon = new TestWebSocketDaemon();

    await manager.registerDaemon('console', consoleDaemon);
    await manager.registerDaemon('websocket', webSocketDaemon);
    await manager.startDaemon('console');
    await manager.startDaemon('websocket');

    // Both should be running
    assert.strictEqual(manager.getDaemon('console')!.isRunning, true);
    assert.strictEqual(manager.getDaemon('websocket')!.isRunning, true);

    // Emergency shutdown
    await manager.emergencyShutdown();

    // Both should be stopped
    assert.strictEqual(manager.getDaemon('console')!.isRunning, false);
    assert.strictEqual(manager.getDaemon('websocket')!.isRunning, false);
  });
});