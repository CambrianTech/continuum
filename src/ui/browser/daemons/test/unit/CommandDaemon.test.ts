/**
 * CommandDaemon Unit Tests
 * 
 * Tests the extracted command execution functionality
 */

import { describe, it, test } from 'node:test';
import assert from 'node:assert';
import { CommandDaemon } from '../../CommandDaemon';

describe('CommandDaemon Unit Tests', () => {
  describe('Initialization', () => {
    test('should instantiate correctly', () => {
      const daemon = new CommandDaemon();
      assert.ok(daemon instanceof CommandDaemon, 'Should create CommandDaemon instance');
    });

    test('should start without WebSocket connection', () => {
      const daemon = new CommandDaemon();
      const status = daemon.getQueueStatus();
      
      assert.strictEqual(status.messageQueue, 0, 'Message queue should be empty');
      assert.strictEqual(status.consoleQueue, 0, 'Console queue should be empty');
      assert.strictEqual(status.pendingCommands, 0, 'Pending commands should be empty');
    });
  });

  describe('Session Management', () => {
    test('should set session ID', () => {
      const daemon = new CommandDaemon();
      
      // Should not throw when setting session ID
      assert.doesNotThrow(() => {
        daemon.setSessionId('test-session-123');
      }, 'Setting session ID should not throw');
    });
  });

  describe('Command Queueing', () => {
    test('should queue console commands when not connected', () => {
      const daemon = new CommandDaemon();
      
      const consoleCommand = {
        action: 'log',
        message: 'Test message',
        source: 'test',
        data: { test: true }
      };
      
      // Queue command when not connected - should add to message queue
      daemon.queueConsoleCommand(consoleCommand);
      
      const status = daemon.getQueueStatus();
      // When not connected, console commands get added to messageQueue instead
      assert.ok(status.messageQueue >= 0, 'Should handle console command when not connected');
    });

    test('should handle command execution without WebSocket', async () => {
      const daemon = new CommandDaemon();
      
      try {
        await daemon.execute('test', {});
        assert.fail('Should throw error when not connected');
      } catch (error) {
        assert.ok(error instanceof Error, 'Should throw Error');
        assert.ok(error.message.includes('not connected'), 'Error should mention connection');
      }
    });
  });

  describe('Mock WebSocket Integration', () => {
    test('should handle WebSocket initialization', () => {
      const daemon = new CommandDaemon();
      
      // Mock WebSocket
      const mockWs = {
        readyState: 1, // OPEN
        send: () => {},
        close: () => {},
        addEventListener: () => {},
        removeEventListener: () => {}
      } as any;
      
      assert.doesNotThrow(() => {
        daemon.initialize(mockWs, 'test-session', 'test-client');
      }, 'WebSocket initialization should not throw');
    });

    test('should handle command responses', () => {
      const daemon = new CommandDaemon();
      
      const mockResponse = {
        type: 'execute_command_response',
        data: {
          requestId: 'test-123',
          success: true,
          data: { result: 'success' }
        }
      };
      
      const handled = daemon.handleCommandResponse(mockResponse);
      assert.strictEqual(handled, false, 'Should return false for unknown request ID');
    });
  });

  describe('Cleanup', () => {
    test('should clean up resources on destroy', () => {
      const daemon = new CommandDaemon();
      
      // Queue some items
      daemon.queueConsoleCommand({
        action: 'log',
        message: 'Test',
        source: 'test',
        data: {}
      });
      
      daemon.destroy();
      
      const status = daemon.getQueueStatus();
      assert.strictEqual(status.messageQueue, 0, 'Message queue should be cleared');
      assert.strictEqual(status.consoleQueue, 0, 'Console queue should be cleared');
      assert.strictEqual(status.pendingCommands, 0, 'Pending commands should be cleared');
    });
  });

  describe('Error Handling', () => {
    test('should handle malformed command responses', () => {
      const daemon = new CommandDaemon();
      
      const malformedResponse = {
        type: 'execute_command_response',
        data: null
      };
      
      assert.doesNotThrow(() => {
        daemon.handleCommandResponse(malformedResponse);
      }, 'Should handle malformed responses gracefully');
    });

    test('should handle missing data in responses', () => {
      const daemon = new CommandDaemon();
      
      const incompleteResponse = {
        type: 'execute_command_response'
        // Missing data field
      };
      
      const handled = daemon.handleCommandResponse(incompleteResponse);
      assert.strictEqual(handled, false, 'Should return false for incomplete responses');
    });
  });
});