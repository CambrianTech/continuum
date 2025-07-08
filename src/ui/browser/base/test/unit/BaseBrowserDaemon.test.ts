/**
 * BaseBrowserDaemon Unit Tests
 * 
 * Tests the foundation browser daemon class following the same
 * patterns as server-side daemon testing.
 */

import { test } from 'node:test';
import assert from 'node:assert';
import { BaseBrowserDaemon, BrowserDaemonMessage, BrowserDaemonResponse } from '../../BaseBrowserDaemon';

// Test implementation of BaseBrowserDaemon
class TestBrowserDaemon extends BaseBrowserDaemon {
  private handleMessageDelay = 0;
  private shouldThrowError = false;

  constructor(name: string = 'test-daemon') {
    super(name, '1.0.0');
  }

  getMessageTypes(): string[] {
    return ['test.message', 'test.another'];
  }

  async handleMessage(message: BrowserDaemonMessage): Promise<BrowserDaemonResponse> {
    if (this.shouldThrowError) {
      throw new Error('Test error handling');
    }

    if (this.handleMessageDelay > 0) {
      await new Promise(resolve => setTimeout(resolve, this.handleMessageDelay));
    }

    return this.createSuccessResponse({
      messageType: message.type,
      received: true,
      timestamp: message.timestamp
    });
  }

  // Test helpers
  setHandleMessageDelay(delay: number): void {
    this.handleMessageDelay = delay;
  }

  setShouldThrowError(shouldThrow: boolean): void {
    this.shouldThrowError = shouldThrow;
  }

  // Expose protected methods for testing
  public testCreateResponse(success: boolean, data?: any, error?: string): BrowserDaemonResponse {
    return this.createResponse(success, data, error);
  }

  public testCreateErrorResponse(error: string): BrowserDaemonResponse {
    return this.createErrorResponse(error);
  }

  public testCreateSuccessResponse(data?: any): BrowserDaemonResponse {
    return this.createSuccessResponse(data);
  }
}

test('BaseBrowserDaemon Lifecycle', async (t) => {
  await t.test('should initialize with correct metadata', () => {
    const daemon = new TestBrowserDaemon('test-daemon');
    const metadata = daemon.getMetadata();

    assert.strictEqual(metadata.name, 'test-daemon');
    assert.strictEqual(metadata.version, '1.0.0');
    assert.strictEqual(metadata.isRunning, false);
    assert.deepStrictEqual(metadata.messageTypes, ['test.message', 'test.another']);
  });

  await t.test('should start and stop correctly', async () => {
    const daemon = new TestBrowserDaemon();

    // Initially not running
    assert.strictEqual(daemon.getIsRunning(), false);

    // Start daemon
    await daemon.start();
    assert.strictEqual(daemon.getIsRunning(), true);

    // Stop daemon
    await daemon.stop();
    assert.strictEqual(daemon.getIsRunning(), false);
  });

  await t.test('should handle double start gracefully', async () => {
    const daemon = new TestBrowserDaemon();

    await daemon.start();
    assert.strictEqual(daemon.getIsRunning(), true);

    // Second start should not throw
    await daemon.start();
    assert.strictEqual(daemon.getIsRunning(), true);

    await daemon.stop();
  });

  await t.test('should handle double stop gracefully', async () => {
    const daemon = new TestBrowserDaemon();

    await daemon.start();
    await daemon.stop();
    assert.strictEqual(daemon.getIsRunning(), false);

    // Second stop should not throw
    await daemon.stop();
    assert.strictEqual(daemon.getIsRunning(), false);
  });
});

test('BaseBrowserDaemon Message Handling', async (t) => {
  await t.test('should handle messages correctly', async () => {
    const daemon = new TestBrowserDaemon();
    await daemon.start();

    const message: BrowserDaemonMessage = {
      type: 'test.message',
      data: { test: 'data' },
      timestamp: new Date().toISOString()
    };

    const response = await daemon.handleMessage(message);

    assert.strictEqual(response.success, true);
    assert.strictEqual(response.data.messageType, 'test.message');
    assert.strictEqual(response.data.received, true);
    assert.ok(response.timestamp);

    await daemon.stop();
  });

  await t.test('should handle message errors gracefully', async () => {
    const daemon = new TestBrowserDaemon();
    daemon.setShouldThrowError(true);
    await daemon.start();

    const message: BrowserDaemonMessage = {
      type: 'test.message',
      data: {},
      timestamp: new Date().toISOString()
    };

    try {
      await daemon.handleMessage(message);
      assert.fail('Should have thrown error');
    } catch (error) {
      assert.ok(error instanceof Error);
      assert.strictEqual(error.message, 'Test error handling');
    }

    await daemon.stop();
  });
});

test('BaseBrowserDaemon Response Creation', async (t) => {
  await t.test('should create success responses correctly', () => {
    const daemon = new TestBrowserDaemon();
    const response = daemon.testCreateSuccessResponse({ test: 'data' });

    assert.strictEqual(response.success, true);
    assert.deepStrictEqual(response.data, { test: 'data' });
    assert.strictEqual(response.error, undefined);
    assert.ok(response.timestamp);
  });

  await t.test('should create error responses correctly', () => {
    const daemon = new TestBrowserDaemon();
    const response = daemon.testCreateErrorResponse('Test error');

    assert.strictEqual(response.success, false);
    assert.strictEqual(response.data, undefined);
    assert.strictEqual(response.error, 'Test error');
    assert.ok(response.timestamp);
  });

  await t.test('should create custom responses correctly', () => {
    const daemon = new TestBrowserDaemon();
    const response = daemon.testCreateResponse(true, { custom: 'data' }, 'warning');

    assert.strictEqual(response.success, true);
    assert.deepStrictEqual(response.data, { custom: 'data' });
    assert.strictEqual(response.error, 'warning');
    assert.ok(response.timestamp);
  });
});

test('BaseBrowserDaemon Message Types', async (t) => {
  await t.test('should declare message types correctly', () => {
    const daemon = new TestBrowserDaemon();
    const messageTypes = daemon.getMessageTypes();

    assert.deepStrictEqual(messageTypes, ['test.message', 'test.another']);
  });

  await t.test('should include message types in metadata', () => {
    const daemon = new TestBrowserDaemon();
    const metadata = daemon.getMetadata();

    assert.deepStrictEqual(metadata.messageTypes, ['test.message', 'test.another']);
  });
});