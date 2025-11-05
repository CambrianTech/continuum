/**
 * Unit Tests for RendererDaemon  
 * Tests isolated daemon functionality without external dependencies
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { RendererDaemon } from '../../RendererDaemon';

describe('RendererDaemon Unit Tests', () => {
  let daemon: RendererDaemon;

  before(async () => {
    daemon = new RendererDaemon();
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
      assert.strictEqual(daemon.name, 'renderer');
      assert.strictEqual(daemon.version, '1.0.0');
    });

    it('should stop successfully', async () => {
      await daemon.stop();
      assert.strictEqual(daemon.isRunning(), false);
    });
  });

  describe('Render Request Handling', () => {
    before(async () => {
      await daemon.start();
    });

    after(async () => {
      await daemon.stop();
    });

    it('should handle render_ui request', async () => {
      const message = {
        id: 'test-1',
        type: 'render_request',
        from: 'test',
        to: 'renderer',
        timestamp: new Date(),
        data: {
          type: 'render_ui',
          data: {
            template: 'main',
            context: {
              title: 'Test Page',
              widgets: ['chat', 'sidebar']
            }
          }
        }
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, true);
      assert(response.data);
    });

    it('should handle render_page request', async () => {
      const message = {
        id: 'test-2', 
        type: 'render_request',
        from: 'test',
        to: 'renderer',
        timestamp: new Date(),
        data: {
          type: 'render_page',
          data: {
            page: 'dashboard',
            clientId: 'test-client-123'
          }
        }
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, true);
      assert(response.data);
    });

    it('should handle update_component request', async () => {
      const message = {
        id: 'test-3',
        type: 'render_request', 
        from: 'test',
        to: 'renderer',
        timestamp: new Date(),
        data: {
          type: 'update_component',
          data: {
            component: 'chat-widget',
            updates: {
              messages: ['Hello', 'World']
            }
          }
        }
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, true);
      assert(response.data);
    });
  });

  describe('HTTP Request Handling', () => {
    before(async () => {
      await daemon.start();
    });

    after(async () => {
      await daemon.stop();
    });

    it('should handle HTTP requests', async () => {
      const message = {
        id: 'test-4',
        type: 'http_request',
        from: 'test',
        to: 'renderer', 
        timestamp: new Date(),
        data: {
          method: 'GET',
          url: '/',
          headers: {}
        }
      };

      const response = await daemon['handleMessage'](message);
      
      // Should either succeed or gracefully handle
      assert(typeof response.success === 'boolean');
      if (response.success) {
        assert(response.data);
      } else {
        assert(response.error);
      }
    });
  });

  describe('Capability Management', () => {
    before(async () => {
      await daemon.start();
    });

    after(async () => {
      await daemon.stop();
    });

    it('should report capabilities', async () => {
      const message = {
        id: 'test-5',
        type: 'get_capabilities',
        from: 'test',
        to: 'renderer',
        timestamp: new Date(),
        data: {}
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, true);
      assert(response.data);
      assert.strictEqual(response.data.ui_generation, true);
      assert.strictEqual(response.data.typescript_compilation, true);
      assert.strictEqual(response.data.template_rendering, true);
      assert.strictEqual(response.data.version_management, true);
    });

    it('should handle unknown message type gracefully', async () => {
      const message = {
        id: 'test-6',
        type: 'unknown_message_type',
        from: 'test',
        to: 'renderer',
        timestamp: new Date(),
        data: {}
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, false);
      assert(response.error);
      assert(response.error.includes('Unknown message type'));
    });
  });

  describe('Daemon Status', () => {
    before(async () => {
      await daemon.start();
    });

    after(async () => {
      await daemon.stop();
    });

    it('should provide status information', async () => {
      const status = daemon.getStatus();
      assert(status);
      assert.strictEqual(status.name, 'renderer');
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

    it('should handle invalid render requests gracefully', async () => {
      const message = {
        id: 'error-test-1',
        type: 'render_request',
        from: 'test',
        to: 'renderer',
        timestamp: new Date(),
        data: {
          type: 'invalid_render_type',
          data: {}
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