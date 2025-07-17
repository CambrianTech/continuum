/**
 * Logger Daemon Tests - Test ProcessBasedDaemon implementation
 * Tests the async queue processing and message handling
 */

import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert';
import { LoggerDaemon } from '../LoggerDaemon';
import { LoggerMessageFactory } from '../LoggerMessageTypes';
import { continuumContextFactory } from '../../../types/shared/core/ContinuumTypes';

describe('LoggerDaemon', () => {
  let daemon: LoggerDaemon;
  let mockContext: any;

  beforeEach(() => {
    daemon = new LoggerDaemon();
    mockContext = continuumContextFactory.create({
      sessionId: 'test-session-123',
      environment: 'server'
    });
    
    // Reset any state as needed
  });

  afterEach(async () => {
    await daemon.stop();
  });

  describe('Daemon Properties', () => {
    it('should have correct daemon properties', () => {
      assert.strictEqual(daemon.name, 'logger');
      assert.strictEqual(daemon.version, '1.0.0');
      assert.strictEqual(daemon.daemonType, 'logger');
    });
  });

  describe('Message Processing', () => {
    it('should process log messages', async () => {
      const logEntry = {
        level: 'info' as const,
        message: 'Test log message',
        timestamp: new Date().toISOString(),
        source: 'test-source',
        context: mockContext
      };

      const message = LoggerMessageFactory.createLogMessage(
        'test-source',
        'logger',
        logEntry
      );

      const response = await daemon['processMessage'](message);

      assert.strictEqual(response.success, true);
      assert.strictEqual(response.messageId, message.id);
      assert.ok(typeof response.processingTime === 'number' && response.processingTime >= 0);
    });

    it('should process flush messages', async () => {
      const message = LoggerMessageFactory.createFlushMessage(
        'test-source',
        'logger',
        { sessionId: 'test-session-123' }
      );

      const response = await daemon['processMessage'](message);

      assert.strictEqual(response.success, true);
      assert.strictEqual(response.messageId, message.id);
    });

    it('should process configure messages', async () => {
      const message = LoggerMessageFactory.createConfigureMessage(
        'test-source',
        'logger',
        { batchSize: 50, enableBatching: true }
      );

      const response = await daemon['processMessage'](message);

      assert.strictEqual(response.success, true);
      assert.strictEqual(response.messageId, message.id);
    });

    it('should handle unknown message types', async () => {
      const message = {
        id: 'test-id',
        from: 'test-source',
        to: 'logger',
        type: 'unknown',
        data: {
          type: 'unknown' as any,
          payload: {}
        },
        timestamp: new Date(),
        priority: 'normal' as const
      };

      const response = await daemon['processMessage'](message);

      assert.strictEqual(response.success, false);
      assert.ok(response.error?.includes('Unknown message type'));
    });
  });

  describe('Batch Processing', () => {
    it('should process multiple log messages in batch', async () => {
      const logEntries = [
        {
          level: 'info' as const,
          message: 'Message 1',
          timestamp: new Date().toISOString(),
          source: 'test-source',
          context: mockContext
        },
        {
          level: 'warn' as const,
          message: 'Message 2',
          timestamp: new Date().toISOString(),
          source: 'test-source',
          context: mockContext
        }
      ];

      const messages = logEntries.map(entry =>
        LoggerMessageFactory.createLogMessage('test-source', 'logger', entry)
      );

      const responses = await daemon['processBatch'](messages);

      assert.strictEqual(responses.length, 2);
      assert.strictEqual(responses[0].success, true);
      assert.strictEqual(responses[1].success, true);
    });

    it('should handle mixed message types in batch', async () => {
      const logEntry = {
        level: 'info' as const,
        message: 'Test message',
        timestamp: new Date().toISOString(),
        source: 'test-source',
        context: mockContext
      };

      const logMessage = LoggerMessageFactory.createLogMessage(
        'test-source',
        'logger',
        logEntry
      );

      const flushMessage = LoggerMessageFactory.createFlushMessage(
        'test-source',
        'logger',
        {}
      );

      const responses = await daemon['processBatch']([logMessage, flushMessage]);

      assert.strictEqual(responses.length, 2);
      assert.strictEqual(responses[0].success, true);
      assert.strictEqual(responses[1].success, true);
    });
  });

  describe('Core Functionality', () => {
    it('should have proper daemon configuration', () => {
      assert.strictEqual(daemon.name, 'logger');
      assert.strictEqual(daemon.daemonType, 'logger');
      assert.strictEqual(daemon.version, '1.0.0');
    });
  });

  describe('Queue Status', () => {
    it('should return queue status', () => {
      const status = daemon.getQueueStatus();

      assert.ok('size' in status);
      assert.ok('isProcessing' in status);
      assert.ok('maxSize' in status);
      assert.strictEqual(typeof status.size, 'number');
      assert.strictEqual(typeof status.isProcessing, 'boolean');
      assert.strictEqual(typeof status.maxSize, 'number');
    });
  });

  describe('Daemon Lifecycle', () => {
    it('should start and stop successfully', async () => {
      await daemon.start();
      assert.strictEqual(daemon['isProcessing'], true);

      await daemon.stop();
      assert.strictEqual(daemon['shutdownSignal'], true);
    });

    it('should flush buffers on shutdown', async () => {
      await daemon.start();
      
      // Add some log entries to buffer
      const logEntry = {
        level: 'info' as const,
        message: 'Test message',
        timestamp: new Date().toISOString(),
        source: 'test-source',
        context: mockContext
      };

      await daemon['addToBuffer'](logEntry);
      
      await daemon.stop();

      // Should have stopped successfully
      assert.strictEqual(daemon['shutdownSignal'], true);
    });
  });

  describe('Error Handling', () => {
    it('should handle unknown message types gracefully', async () => {
      const message = {
        id: 'test-id',
        from: 'test-source',
        to: 'logger',
        type: 'unknown',
        data: {
          type: 'unknown' as any,
          payload: {}
        },
        timestamp: new Date(),
        priority: 'normal' as const
      };

      const response = await daemon['processMessage'](message);

      assert.strictEqual(response.success, false);
      assert.ok(response.error?.includes('Unknown message type'));
    });
  });
});