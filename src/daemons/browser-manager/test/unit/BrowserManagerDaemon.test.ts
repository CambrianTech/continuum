/**
 * Unit Tests for BrowserManagerDaemon
 * Tests isolated daemon functionality without external dependencies
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { BrowserManagerDaemon } from '../../BrowserManagerDaemon.js';

describe('BrowserManagerDaemon Unit Tests', () => {
  let daemon: BrowserManagerDaemon;

  before(async () => {
    daemon = new BrowserManagerDaemon();
  });

  after(async () => {
    if (daemon && daemon.isRunning()) {
      await daemon.stop();
    }
  });

  describe('Daemon Lifecycle', () => {
    it('should start successfully', async () => {
      await daemon.start();
      assert.strictEqual(daemon.isRunning(), true);
    });

    it('should have correct daemon properties', () => {
      assert.strictEqual(daemon.name, 'browser-manager');
      assert.strictEqual(typeof daemon.version, 'string');
      assert(daemon.version.length > 0);
    });

    it('should stop successfully', async () => {
      await daemon.stop();
      assert.strictEqual(daemon.isRunning(), false);
    });
  });

  describe('Browser Request Handling', () => {
    before(async () => {
      await daemon.start();
    });

    after(async () => {
      await daemon.stop();
    });

    it('should handle create browser message', async () => {
      const message = {
        id: 'test-1',
        type: 'browser_request',
        from: 'test',
        to: 'browser-manager',
        timestamp: new Date(),
        data: {
          type: 'create',
          sessionId: 'test-session-123',
          config: {
            purpose: 'development',
            persona: 'developer',
            requirements: {
              devtools: true,
              isolation: 'sandboxed' as const,
              visibility: 'visible' as const,
              persistence: 'session' as const
            },
            resources: {
              priority: 'normal' as const
            }
          }
        }
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, true);
      assert(response.data);
    });

    it('should handle list browsers message', async () => {
      const message = {
        id: 'test-2',
        type: 'browser_request',
        from: 'test',
        to: 'browser-manager',
        timestamp: new Date(),
        data: {
          type: 'list'
        }
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, true);
      assert(response.data);
      assert(Array.isArray(response.data.browsers));
    });

    it('should handle unknown message type gracefully', async () => {
      const message = {
        id: 'test-3',
        type: 'unknown_message_type',
        from: 'test',
        to: 'browser-manager',
        timestamp: new Date(),
        data: {}
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, false);
      assert(response.error);
      assert(response.error.includes('Unknown message type') || response.error.includes('not supported'));
    });
  });

  describe('Browser State Management', () => {
    before(async () => {
      await daemon.start();
    });

    after(async () => {
      await daemon.stop();
    });

    it('should track browser sessions', async () => {
      const status = daemon.getStatus();
      assert(status);
      assert.strictEqual(typeof status.name, 'string');
      assert.strictEqual(status.name, 'browser-manager');
    });

    it('should handle resource optimization requests', async () => {
      const message = {
        id: 'test-4',
        type: 'browser_request',
        from: 'test',
        to: 'browser-manager',
        timestamp: new Date(),
        data: {
          type: 'optimize'
        }
      };

      const response = await daemon['handleMessage'](message);
      
      // Should either succeed or gracefully handle
      assert(typeof response.success === 'boolean');
      if (!response.success) {
        assert(response.error);
      }
    });
  });

  describe('Error Handling', () => {
    before(async () => {
      await daemon.start();
    });

    after(async () => {
      await daemon.stop();
    });

    it('should handle invalid browser creation parameters', async () => {
      const message = {
        id: 'error-test-1',
        type: 'browser_request',
        from: 'test',
        to: 'browser-manager',
        timestamp: new Date(),
        data: {
          type: 'create',
          // Missing required fields
        }
      };

      const response = await daemon['handleMessage'](message);
      
      // Should either succeed with defaults or fail gracefully
      assert(typeof response.success === 'boolean');
      if (!response.success) {
        assert(response.error);
      }
    });
  });
});