/**
 * Console Overrides Tests - Test console interception and async routing
 * Tests the integration between console methods and async logger
 */

import { describe, it, beforeEach, afterEach, expect, vi } from 'vitest';
import { ServerConsoleOverrides } from '../server/ConsoleOverrides';

// Mock logger client
vi.mock('../../daemons/logger/LoggerClient', () => ({
  loggerClient: {
    initialize: vi.fn(),
    log: vi.fn(),
    flush: vi.fn(),
    shutdown: vi.fn()
  }
}));

// Mock SessionContext
vi.mock('../../context/SessionContext', () => ({
  SessionContext: {
    getCurrentSessionSync: vi.fn(() => 'test-session-123')
  }
}));

describe('ServerConsoleOverrides', () => {
  let consoleOverrides: ServerConsoleOverrides;
  let originalConsole: {
    log: typeof console.log;
    info: typeof console.info;
    warn: typeof console.warn;
    error: typeof console.error;
    debug: typeof console.debug;
  };

  beforeEach(() => {
    // Store original console methods
    originalConsole = {
      log: console.log,
      info: console.info,
      warn: console.warn,
      error: console.error,
      debug: console.debug
    };

    consoleOverrides = ServerConsoleOverrides.getInstance();
  });

  afterEach(async () => {
    await consoleOverrides.shutdown();
    
    // Restore original console methods
    console.log = originalConsole.log;
    console.info = originalConsole.info;
    console.warn = originalConsole.warn;
    console.error = originalConsole.error;
    console.debug = originalConsole.debug;
  });

  describe('Initialization', () => {
    it('should be a singleton', () => {
      const instance1 = ServerConsoleOverrides.getInstance();
      const instance2 = ServerConsoleOverrides.getInstance();
      
      expect(instance1).toBe(instance2);
    });

    it('should initialize async logger', async () => {
      await consoleOverrides.initialize();
      
      // Should have overridden console methods
      expect(console.log).not.toBe(originalConsole.log);
      expect(console.info).not.toBe(originalConsole.info);
      expect(console.warn).not.toBe(originalConsole.warn);
      expect(console.error).not.toBe(originalConsole.error);
      expect(console.debug).not.toBe(originalConsole.debug);
    });
  });

  describe('Console Method Overrides', () => {
    beforeEach(async () => {
      await consoleOverrides.initialize();
    });

    it('should override console.log', () => {
      const spy = vi.spyOn(originalConsole, 'log');
      
      console.log('test message');
      
      expect(spy).toHaveBeenCalledWith('test message');
    });

    it('should override console.info', () => {
      const spy = vi.spyOn(originalConsole, 'info');
      
      console.info('test info');
      
      expect(spy).toHaveBeenCalledWith('test info');
    });

    it('should override console.warn', () => {
      const spy = vi.spyOn(originalConsole, 'warn');
      
      console.warn('test warning');
      
      expect(spy).toHaveBeenCalledWith('test warning');
    });

    it('should override console.error', () => {
      const spy = vi.spyOn(originalConsole, 'error');
      
      console.error('test error');
      
      expect(spy).toHaveBeenCalledWith('test error');
    });

    it('should override console.debug', () => {
      const spy = vi.spyOn(originalConsole, 'debug');
      
      console.debug('test debug');
      
      expect(spy).toHaveBeenCalledWith('test debug');
    });
  });

  describe('Async Logging Integration', () => {
    beforeEach(async () => {
      await consoleOverrides.initialize();
    });

    it('should handle multiple arguments', () => {
      const spy = vi.spyOn(originalConsole, 'log');
      
      console.log('Message with', 'multiple', 'arguments', { key: 'value' });
      
      expect(spy).toHaveBeenCalledWith(
        'Message with',
        'multiple', 
        'arguments',
        { key: 'value' }
      );
    });

    it('should handle objects in console calls', () => {
      const spy = vi.spyOn(originalConsole, 'log');
      const testObj = { user: 'test', action: 'login' };
      
      console.log('User action:', testObj);
      
      expect(spy).toHaveBeenCalledWith('User action:', testObj);
    });

    it('should not block console calls', async () => {
      const startTime = Date.now();
      
      console.log('This should not block');
      
      const endTime = Date.now();
      const duration = endTime - startTime;
      
      // Should complete very quickly (< 10ms)
      expect(duration).toBeLessThan(10);
    });
  });

  describe('Error Handling', () => {
    beforeEach(async () => {
      await consoleOverrides.initialize();
    });

    it('should handle errors in async logging without crashing', () => {
      // Mock SessionContext to throw error
      const { SessionContext } = require('../../context/SessionContext');
      vi.mocked(SessionContext.getCurrentSessionSync).mockImplementationOnce(() => {
        throw new Error('SessionContext error');
      });

      // Should not throw error
      expect(() => {
        console.log('test message');
      }).not.toThrow();
    });

    it('should handle logger errors gracefully', async () => {
      const { loggerClient } = require('../../daemons/logger/LoggerClient');
      vi.mocked(loggerClient.log).mockRejectedValueOnce(new Error('Logger error'));

      // Should not throw error
      expect(() => {
        console.log('test message');
      }).not.toThrow();
    });
  });

  describe('Restoration', () => {
    it('should restore original console methods', async () => {
      await consoleOverrides.initialize();
      
      // Console should be overridden
      expect(console.log).not.toBe(originalConsole.log);
      
      consoleOverrides.restore();
      
      // Console should be restored
      expect(console.log).toBe(originalConsole.log);
      expect(console.info).toBe(originalConsole.info);
      expect(console.warn).toBe(originalConsole.warn);
      expect(console.error).toBe(originalConsole.error);
      expect(console.debug).toBe(originalConsole.debug);
    });

    it('should restore console methods on shutdown', async () => {
      await consoleOverrides.initialize();
      
      expect(console.log).not.toBe(originalConsole.log);
      
      await consoleOverrides.shutdown();
      
      expect(console.log).toBe(originalConsole.log);
    });
  });
});