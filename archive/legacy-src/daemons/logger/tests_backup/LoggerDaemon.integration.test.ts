/**
 * Logger Daemon Integration Tests
 * Tests ProcessBasedDaemon async queue processing and file operations
 */

import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert';
import { LoggerDaemon } from '../LoggerDaemon';
import { LoggerMessageFactory } from '../LoggerMessageTypes';
import { continuumContextFactory } from '../../../types/shared/core/ContinuumTypes';
import * as fs from 'fs/promises';
import * as path from 'path';

describe('LoggerDaemon Integration Tests', () => {
  let daemon: LoggerDaemon;
  let mockContext: any;
  let testSessionId: string;
  let testLogDir: string;

  beforeEach(async () => {
    testSessionId = 'test-session-' + Date.now();
    testLogDir = path.join(process.cwd(), '.continuum', 'sessions', 'user', 'shared', testSessionId, 'logs');
    
    daemon = new LoggerDaemon();
    mockContext = continuumContextFactory.create({
      sessionId: testSessionId as any,
      environment: 'server'
    });
  });

  afterEach(async () => {
    await daemon.stop();
    
    // Clean up test files
    try {
      await fs.rm(testLogDir, { recursive: true, force: true });
    } catch (error) {
      // Ignore cleanup errors
    }
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
      assert.ok(typeof response.processingTime === 'number');
    });

    it('should handle flush messages', async () => {
      const message = LoggerMessageFactory.createFlushMessage(
        'test-source',
        'logger',
        { sessionId: testSessionId }
      );

      const response = await daemon['processMessage'](message);

      assert.strictEqual(response.success, true);
      assert.strictEqual(response.messageId, message.id);
    });

    it('should handle configure messages', async () => {
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

  describe('Queue Status', () => {
    it('should return queue status', () => {
      const status = daemon.getQueueStatus();

      assert.ok(typeof status.size === 'number');
      assert.ok(typeof status.isProcessing === 'boolean');
      assert.ok(typeof status.maxSize === 'number');
    });
  });

  describe('Daemon Lifecycle', () => {
    it('should start and stop successfully', async () => {
      await daemon.start();
      assert.strictEqual(daemon['isProcessing'], true);

      await daemon.stop();
      assert.strictEqual(daemon['shutdownSignal'], true);
    });
  });
});