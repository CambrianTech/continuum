/**
 * Async Logger Tests - Test suite for async logging functionality
 * Tests the ProcessBasedDaemon logger integration
 */

import { describe, it, beforeEach, afterEach, expect, vi } from 'vitest';
import { ServerAsyncLogger } from '../server/AsyncLogger';
import { continuumContextFactory } from '../../types/shared/core/ContinuumTypes';
import { loggerClient } from '../../daemons/logger/LoggerClient';

// Mock the logger client
vi.mock('../../daemons/logger/LoggerClient', () => ({
  loggerClient: {
    initialize: vi.fn(),
    log: vi.fn(),
    flush: vi.fn(),
    shutdown: vi.fn()
  }
}));

describe('ServerAsyncLogger', () => {
  let logger: ServerAsyncLogger;
  let mockContext: any;

  beforeEach(() => {
    logger = new ServerAsyncLogger();
    mockContext = continuumContextFactory.create({
      environment: 'server'
    });
    
    // Reset mocks
    vi.clearAllMocks();
  });

  afterEach(async () => {
    await logger.shutdown();
  });

  describe('Initialization', () => {
    it('should initialize logger client on first use', async () => {
      await logger.log(mockContext, 'info', 'test message');
      
      expect(loggerClient.initialize).toHaveBeenCalledTimes(1);
    });

    it('should not initialize multiple times', async () => {
      await logger.log(mockContext, 'info', 'test message 1');
      await logger.log(mockContext, 'info', 'test message 2');
      
      expect(loggerClient.initialize).toHaveBeenCalledTimes(1);
    });
  });

  describe('Logging Methods', () => {
    it('should log info messages', async () => {
      await logger.info(mockContext, 'info message', 'test-source');
      
      expect(loggerClient.log).toHaveBeenCalledWith(
        mockContext,
        'info',
        'info message',
        'test-source',
        undefined
      );
    });

    it('should log debug messages', async () => {
      await logger.debug(mockContext, 'debug message', 'test-source');
      
      expect(loggerClient.log).toHaveBeenCalledWith(
        mockContext,
        'debug',
        'debug message',
        'test-source',
        undefined
      );
    });

    it('should log warn messages', async () => {
      await logger.warn(mockContext, 'warn message', 'test-source');
      
      expect(loggerClient.log).toHaveBeenCalledWith(
        mockContext,
        'warn',
        'warn message',
        'test-source',
        undefined
      );
    });

    it('should log error messages', async () => {
      await logger.error(mockContext, 'error message', 'test-source');
      
      expect(loggerClient.log).toHaveBeenCalledWith(
        mockContext,
        'error',
        'error message',
        'test-source',
        undefined
      );
    });

    it('should include metadata when provided', async () => {
      const metadata = { userId: 'test-123', action: 'login' };
      
      await logger.info(mockContext, 'user logged in', 'auth-service', metadata);
      
      expect(loggerClient.log).toHaveBeenCalledWith(
        mockContext,
        'info',
        'user logged in',
        'auth-service',
        metadata
      );
    });

    it('should use default source when not provided', async () => {
      await logger.info(mockContext, 'message without source');
      
      expect(loggerClient.log).toHaveBeenCalledWith(
        mockContext,
        'info',
        'message without source',
        'unknown',
        undefined
      );
    });
  });

  describe('Flush and Shutdown', () => {
    it('should flush logs', async () => {
      await logger.flush('test-session-id');
      
      expect(loggerClient.flush).toHaveBeenCalledWith('test-session-id');
    });

    it('should flush all logs when no session provided', async () => {
      await logger.flush();
      
      expect(loggerClient.flush).toHaveBeenCalledWith(undefined);
    });

    it('should shutdown logger client', async () => {
      await logger.shutdown();
      
      expect(loggerClient.shutdown).toHaveBeenCalled();
    });
  });

  describe('Error Handling', () => {
    it('should handle logger client errors gracefully', async () => {
      vi.mocked(loggerClient.log).mockRejectedValueOnce(new Error('Logger daemon down'));
      
      await expect(logger.info(mockContext, 'test message')).rejects.toThrow('Logger daemon down');
    });

    it('should handle initialization errors', async () => {
      vi.mocked(loggerClient.initialize).mockRejectedValueOnce(new Error('Init failed'));
      
      await expect(logger.info(mockContext, 'test message')).rejects.toThrow('Init failed');
    });
  });

  describe('Configuration', () => {
    it('should accept configuration options', () => {
      const config = {
        enableBatching: true,
        batchSize: 50,
        flushInterval: 1000
      };
      
      const configuredLogger = new ServerAsyncLogger(config);
      
      expect(configuredLogger).toBeDefined();
      // Configuration is stored but not directly testable without exposing internals
    });
  });
});