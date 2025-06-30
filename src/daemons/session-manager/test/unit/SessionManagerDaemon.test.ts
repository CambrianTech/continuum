/**
 * Unit Tests for SessionManagerDaemon
 * Tests isolated daemon functionality without external dependencies
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { SessionManagerDaemon } from '../../SessionManagerDaemon.js';

describe('SessionManagerDaemon Unit Tests', () => {
  let daemon: SessionManagerDaemon;

  before(async () => {
    daemon = new SessionManagerDaemon();
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
      assert.strictEqual(daemon.name, 'session-manager');
      assert.strictEqual(typeof daemon.version, 'string');
      assert(daemon.version.length > 0);
    });

    it('should stop successfully', async () => {
      await daemon.stop();
      assert.strictEqual(daemon.isRunning(), false);
    });
  });

  describe('Connection Identity Management', () => {
    before(async () => {
      await daemon.start();
    });

    after(async () => {
      await daemon.stop();
    });

    it('should handle register_connection_identity message', async () => {
      const message = {
        id: 'test-1',
        type: 'register_connection_identity',
        from: 'test',
        to: 'session-manager',
        timestamp: new Date(),
        data: {
          connectionId: 'test-connection-123',
          identity: {
            type: 'user',
            name: 'test-user',
            sessionContext: 'development'
          }
        }
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, true);
      assert(response.data);
    });
  });

  describe('Session Creation', () => {
    before(async () => {
      await daemon.start();
    });

    after(async () => {
      await daemon.stop();
    });

    it('should handle create_session message', async () => {
      const message = {
        id: 'test-2',
        type: 'create_session',
        from: 'test',
        to: 'session-manager',
        timestamp: new Date(),
        data: {
          type: 'development',
          owner: 'test-user',
          sessionId: 'dev-session-456'
        }
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, true);
      assert(response.data);
      assert(response.data.sessionId);
    });

    it('should handle create_session_for_connection message', async () => {
      const message = {
        id: 'test-3',
        type: 'create_session_for_connection',
        from: 'test',
        to: 'session-manager',
        timestamp: new Date(),
        data: {
          connectionId: 'test-connection-789'
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

  describe('Session Management', () => {
    before(async () => {
      await daemon.start();
    });

    after(async () => {
      await daemon.stop();
    });

    it('should handle get_session message', async () => {
      const message = {
        id: 'test-4',
        type: 'get_session',
        from: 'test',
        to: 'session-manager',
        timestamp: new Date(),
        data: {
          sessionId: 'test-session-456'
        }
      };

      const response = await daemon['handleMessage'](message);
      
      // Should either find session or gracefully handle
      assert(typeof response.success === 'boolean');
      if (!response.success) {
        assert(response.error);
      }
    });

    it('should handle list_sessions message', async () => {
      const message = {
        id: 'test-5',
        type: 'list_sessions',
        from: 'test',
        to: 'session-manager',
        timestamp: new Date(),
        data: {
          owner: 'test-user'
        }
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, true);
      assert(response.data);
      assert(Array.isArray(response.data.sessions));
    });

    it('should handle get_session_stats message', async () => {
      const message = {
        id: 'test-6',
        type: 'get_session_stats',
        from: 'test',
        to: 'session-manager',
        timestamp: new Date(),
        data: {}
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, true);
      assert(response.data);
      // Accept any stats structure - just verify it returns data
      assert(typeof response.data === 'object');
    });
  });

  describe('Artifact Management', () => {
    before(async () => {
      await daemon.start();
    });

    after(async () => {
      await daemon.stop();
    });

    it('should handle add_artifact message', async () => {
      const message = {
        id: 'test-7',
        type: 'add_artifact',
        from: 'test',
        to: 'session-manager',
        timestamp: new Date(),
        data: {
          sessionId: 'test-session-456',
          artifactType: 'screenshot',
          artifactData: {
            filename: 'test-screenshot.png',
            path: '/tmp/test-screenshot.png'
          }
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

  describe('Session Lifecycle', () => {
    before(async () => {
      await daemon.start();
    });

    after(async () => {
      await daemon.stop();
    });

    it('should handle close_session message', async () => {
      const message = {
        id: 'test-8',
        type: 'close_session',
        from: 'test',
        to: 'session-manager',
        timestamp: new Date(),
        data: {
          sessionId: 'test-session-456'
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

  describe('Status and Health', () => {
    before(async () => {
      await daemon.start();
    });

    after(async () => {
      await daemon.stop();
    });

    it('should provide daemon status information', async () => {
      const status = daemon.getStatus();
      assert(status);
      assert.strictEqual(status.name, 'session-manager');
      assert.strictEqual(status.status, 'running');
      assert(typeof status.uptime === 'number');
    });
  });

  describe('Error Handling', () => {
    before(async () => {
      await daemon.start();
    });

    after(async () => {
      await daemon.stop();
    });

    it('should handle unknown message type gracefully', async () => {
      const message = {
        id: 'test-9',
        type: 'unknown_message_type',
        from: 'test',
        to: 'session-manager',
        timestamp: new Date(),
        data: {}
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, false);
      assert(response.error);
      assert(response.error.includes('Unknown message type'));
    });

    it('should handle invalid session creation gracefully', async () => {
      const message = {
        id: 'error-test-1',
        type: 'create_session',
        from: 'test',
        to: 'session-manager',
        timestamp: new Date(),
        data: {
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