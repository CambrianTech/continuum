/**
 * Unit Tests for PersonaDaemon
 * Tests isolated daemon functionality without external dependencies
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { PersonaDaemon } from '../../PersonaDaemon.js';

describe('PersonaDaemon Unit Tests', () => {
  let daemon: PersonaDaemon;

  before(async () => {
    const testConfig = {
      id: 'test-persona',
      name: 'Test Persona',
      modelProvider: 'openai' as const,
      modelConfig: {
        model: 'gpt-4'
      },
      capabilities: ['chat', 'command_execution'],
      sessionDirectory: '/tmp/test-persona-sessions'
    };
    
    daemon = new PersonaDaemon(testConfig);
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
      assert.strictEqual(daemon.name, 'persona-test-persona');
      assert.strictEqual(daemon.version, '1.0.0');
    });

    it('should stop successfully', async () => {
      await daemon.stop();
      assert.strictEqual(daemon.isRunning(), false);
    });
  });

  describe('Command Execution', () => {
    before(async () => {
      await daemon.start();
    });

    after(async () => {
      await daemon.stop();
    });

    it('should handle execute_command message', async () => {
      const message = {
        id: 'test-1',
        type: 'execute_command',
        from: 'test',
        to: 'persona',
        timestamp: new Date(),
        data: {
          command: 'selftest',
          parameters: { mode: 'simple' },
          personaContext: {
            persona: 'test-persona',
            sessionId: 'test-session-123'
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

  describe('Chat Message Processing', () => {
    before(async () => {
      await daemon.start();
    });

    after(async () => {
      await daemon.stop();
    });

    it('should handle chat_message message', async () => {
      const message = {
        id: 'test-2',
        type: 'chat_message',
        from: 'test',
        to: 'persona',
        timestamp: new Date(),
        data: {
          message: 'Hello, test persona!',
          persona: 'test-persona',
          context: {
            sessionId: 'chat-session-456'
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

  describe('Academy Training', () => {
    before(async () => {
      await daemon.start();
    });

    after(async () => {
      await daemon.stop();
    });

    it('should handle academy_training message', async () => {
      const message = {
        id: 'test-3',
        type: 'academy_training',
        from: 'test',
        to: 'persona',
        timestamp: new Date(),
        data: {
          trainingType: 'adversarial',
          targetPersona: 'test-persona',
          trainingData: {
            commands: ['selftest', 'help'],
            expectedOutputs: ['success', 'usage info']
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

  describe('LoRA Adaptation', () => {
    before(async () => {
      await daemon.start();
    });

    after(async () => {
      await daemon.stop();
    });

    it('should handle lora_adaptation message', async () => {
      const message = {
        id: 'test-4',
        type: 'lora_adaptation',
        from: 'test',
        to: 'persona',
        timestamp: new Date(),
        data: {
          operation: 'apply',
          personaId: 'test-persona',
          adapterConfig: {
            domain: 'development',
            specialization: 'typescript'
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

  describe('Status Management', () => {
    before(async () => {
      await daemon.start();
    });

    after(async () => {
      await daemon.stop();
    });

    it('should handle get_status message', async () => {
      const message = {
        id: 'test-5',
        type: 'get_status',
        from: 'test',
        to: 'persona',
        timestamp: new Date(),
        data: {}
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, true);
      assert(response.data);
    });

    it('should provide daemon status information', async () => {
      const status = daemon.getStatus();
      assert(status);
      assert.strictEqual(status.name, 'persona-test-persona');
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
        id: 'test-6',
        type: 'unknown_message_type',
        from: 'test',
        to: 'persona',
        timestamp: new Date(),
        data: {}
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, false);
      assert(response.error);
      assert(response.error.includes('Unknown message type'));
    });

    it('should handle invalid command execution gracefully', async () => {
      const message = {
        id: 'error-test-1',
        type: 'execute_command',
        from: 'test',
        to: 'persona',
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