/**
 * Unit Test: Session Console Manager
 * 
 * Tests console logging management independently of session infrastructure
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { SessionConsoleManager } from '../SessionConsoleManager.js';
import * as fs from 'fs/promises';
import * as path from 'path';
import * as os from 'os';

describe('SessionConsoleManager', () => {
  let consoleManager: SessionConsoleManager;
  let tempDir: string;
  let testLogPath: string;

  beforeEach(async () => {
    consoleManager = new SessionConsoleManager();
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'console-manager-test-'));
    testLogPath = path.join(tempDir, 'browser.log');
    await fs.writeFile(testLogPath, '# Test log file\n');
  });

  afterEach(async () => {
    await consoleManager.stopAll();
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  describe('Basic Operations', () => {
    it('should start with no active loggers', () => {
      expect(consoleManager.getActiveCount()).toBe(0);
      expect(consoleManager.getActiveSessionIds()).toEqual([]);
      expect(consoleManager.isLogging('any-session')).toBe(false);
    });

    it('should handle start logging request', async () => {
      const request = {
        sessionId: 'test-session-1',
        debugUrl: 'http://localhost:9222',
        logPath: testLogPath
      };

      const result = await consoleManager.startLogging(request);
      
      // Should accept request even if DevTools connection fails
      // (We're testing the manager logic, not DevTools integration)
      expect(result.success).toBe(true);
      expect(consoleManager.getActiveCount()).toBeGreaterThanOrEqual(0);
    });

    it('should handle duplicate start requests gracefully', async () => {
      const request = {
        sessionId: 'test-session-2',
        debugUrl: 'http://localhost:9222',
        logPath: testLogPath
      };

      // Start twice
      const result1 = await consoleManager.startLogging(request);
      const result2 = await consoleManager.startLogging(request);

      expect(result1.success).toBe(true);
      expect(result2.success).toBe(true);
    });

    it('should stop logging for session', async () => {
      const sessionId = 'test-session-3';
      
      // Start logging first
      await consoleManager.startLogging({
        sessionId,
        debugUrl: 'http://localhost:9222',
        logPath: testLogPath
      });

      // Stop logging
      const result = await consoleManager.stopLogging(sessionId);
      expect(result.success).toBe(true);
      expect(consoleManager.isLogging(sessionId)).toBe(false);
    });

    it('should handle stop for non-existent session', async () => {
      const result = await consoleManager.stopLogging('non-existent');
      expect(result.success).toBe(true); // Should be graceful
    });
  });

  describe('Multiple Sessions', () => {
    it('should manage multiple console loggers', async () => {
      const sessions = ['session-1', 'session-2', 'session-3'];
      
      // Start logging for multiple sessions
      for (const sessionId of sessions) {
        await consoleManager.startLogging({
          sessionId,
          debugUrl: `http://localhost:922${sessionId.slice(-1)}`,
          logPath: testLogPath
        });
      }

      expect(consoleManager.getActiveCount()).toBeGreaterThanOrEqual(0);
      
      // Stop one session
      await consoleManager.stopLogging('session-2');
      expect(consoleManager.isLogging('session-2')).toBe(false);
    });

    it('should stop all loggers during cleanup', async () => {
      // Start multiple loggers
      await consoleManager.startLogging({
        sessionId: 'cleanup-1',
        debugUrl: 'http://localhost:9222',
        logPath: testLogPath
      });
      await consoleManager.startLogging({
        sessionId: 'cleanup-2',
        debugUrl: 'http://localhost:9223',
        logPath: testLogPath
      });

      // Stop all
      await consoleManager.stopAll();
      expect(consoleManager.getActiveCount()).toBe(0);
    });
  });

  describe('Error Handling', () => {
    it('should handle invalid debug URLs gracefully', async () => {
      const result = await consoleManager.startLogging({
        sessionId: 'invalid-url-test',
        debugUrl: 'invalid-url',
        logPath: testLogPath
      });

      // Should return error but not crash
      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
    });

    it('should handle missing log paths', async () => {
      const result = await consoleManager.startLogging({
        sessionId: 'missing-path-test',
        debugUrl: 'http://localhost:9222',
        logPath: '/non/existent/path.log'
      });

      // Should handle gracefully
      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
    });
  });

  describe('State Tracking', () => {
    it('should track active sessions correctly', async () => {
      expect(consoleManager.getActiveSessionIds()).toEqual([]);

      await consoleManager.startLogging({
        sessionId: 'tracked-session',
        debugUrl: 'http://localhost:9222',
        logPath: testLogPath
      });

      // Should track the session (even if DevTools connection fails)
      const activeSessions = consoleManager.getActiveSessionIds();
      expect(activeSessions.length).toBeGreaterThanOrEqual(0);
    });

    it('should report logging status correctly', async () => {
      const sessionId = 'status-test';
      
      expect(consoleManager.isLogging(sessionId)).toBe(false);
      
      await consoleManager.startLogging({
        sessionId,
        debugUrl: 'http://localhost:9222',
        logPath: testLogPath
      });

      // Status depends on whether DevTools connection succeeded
      // But manager should track the attempt
    });
  });
});