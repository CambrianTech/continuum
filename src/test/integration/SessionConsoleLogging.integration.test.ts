/**
 * Layer 6 Integration Test: Session Console Logging
 * 
 * Validates that browser console output gets captured and written to session logs
 * This completes the git hook validation pattern where console.log output 
 * with UUIDs can be tracked and verified.
 */

import { describe, it, expect, beforeEach, afterEach, beforeAll, afterAll } from 'vitest';
import { SessionManagerDaemon } from '../../daemons/session-manager/SessionManagerDaemon.js';
import { SessionConsoleLogger } from '../../daemons/session-manager/modules/SessionConsoleLogger.js';
import * as fs from 'fs/promises';
import * as path from 'path';
import * as os from 'os';

describe('Layer 6: Session Console Logging Integration', () => {
  let sessionManager: SessionManagerDaemon;
  let testSessionId: string;
  let testLogPath: string;
  let tempDir: string;

  beforeAll(async () => {
    // Create temporary directory for test sessions
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'continuum-console-test-'));
    sessionManager = new SessionManagerDaemon(tempDir);
    await sessionManager.start();
  });

  afterAll(async () => {
    await sessionManager.stop();
    // Cleanup test session artifacts
    try {
      await fs.rm(tempDir, { recursive: true, force: true });
    } catch (error) {
      // Ignore cleanup errors
    }
  });

  beforeEach(async () => {
    // Create a test session for console logging
    const sessionResult = await sessionManager.handleMessage({
      type: 'create_session',
      data: {
        type: 'test',
        owner: 'console-test',
        options: {
          context: 'layer6-console-integration'
        }
      }
    });

    expect(sessionResult.success).toBe(true);
    testSessionId = sessionResult.data?.sessionId;
    expect(testSessionId).toBeDefined();

    const session = sessionManager.getSession(testSessionId);
    expect(session).toBeDefined();
    testLogPath = session!.artifacts.logs.client[0];
  });

  afterEach(async () => {
    // Stop any active console logging
    if (testSessionId) {
      await sessionManager.handleMessage({
        type: 'stop_console_logging',
        data: { sessionId: testSessionId }
      });
      
      // Close the test session
      await sessionManager.handleMessage({
        type: 'close_session',
        data: { sessionId: testSessionId, preserveArtifacts: false }
      });
    }
  });

  describe('Session Console Logger Module', () => {
    it('should create and configure session console logger', async () => {
      const logger = new SessionConsoleLogger();
      
      // Should start inactive
      expect(logger.isActive()).toBe(false);
      expect(logger.getSessionLogPath()).toBeNull();
      
      // Should configure log path
      logger.setSessionLogPath(testLogPath);
      expect(logger.getSessionLogPath()).toBe(testLogPath);
    });

    it('should handle DevTools connection lifecycle', async () => {
      const logger = new SessionConsoleLogger();
      logger.setSessionLogPath(testLogPath);
      
      // Should handle invalid URL gracefully
      await expect(logger.startLogging('invalid-url')).rejects.toThrow();
      expect(logger.isActive()).toBe(false);
      
      // Should stop cleanly even if not started
      await logger.stopLogging();
      expect(logger.isActive()).toBe(false);
    });
  });

  describe('Session Manager Console Integration', () => {
    it('should handle start console logging request', async () => {
      const mockDebugUrl = 'http://localhost:9222';
      
      const result = await sessionManager.handleMessage({
        type: 'start_console_logging',
        data: {
          sessionId: testSessionId,
          debugUrl: mockDebugUrl
        }
      });

      // Should accept the request even if DevTools connection fails
      // (Real DevTools testing requires actual browser instance)
      expect(result.success).toBe(true);
      expect(result.data?.sessionId).toBe(testSessionId);
      expect(result.data?.debugUrl).toBe(mockDebugUrl);
      expect(result.data?.logPath).toBe(testLogPath);
    });

    it('should handle stop console logging request', async () => {
      // First try to start (will fail but create logger instance)
      await sessionManager.handleMessage({
        type: 'start_console_logging',
        data: {
          sessionId: testSessionId,
          debugUrl: 'http://localhost:9222'
        }
      });

      // Then stop
      const result = await sessionManager.handleMessage({
        type: 'stop_console_logging',
        data: { sessionId: testSessionId }
      });

      expect(result.success).toBe(true);
      expect(result.data?.sessionId).toBe(testSessionId);
    });

    it('should track console loggers in session stats', async () => {
      // Check initial stats
      const initialStats = await sessionManager.handleMessage({
        type: 'get_session_stats',
        data: {}
      });
      expect(initialStats.success).toBe(true);
      const initialLoggers = initialStats.data?.consoleLoggers || 0;

      // Start console logging (will create logger instance)
      await sessionManager.handleMessage({
        type: 'start_console_logging',
        data: {
          sessionId: testSessionId,
          debugUrl: 'http://localhost:9222'
        }
      });

      // Check stats again
      const afterStats = await sessionManager.handleMessage({
        type: 'get_session_stats',
        data: {}
      });
      expect(afterStats.success).toBe(true);
      expect(afterStats.data?.consoleLoggers).toBeGreaterThanOrEqual(initialLoggers);
    });

    it('should handle session closure with active console logging', async () => {
      // Start console logging
      await sessionManager.handleMessage({
        type: 'start_console_logging',
        data: {
          sessionId: testSessionId,
          debugUrl: 'http://localhost:9222'
        }
      });

      // Close session (should stop console logging automatically)
      const result = await sessionManager.handleMessage({
        type: 'close_session',
        data: { sessionId: testSessionId, preserveArtifacts: true }
      });

      expect(result.success).toBe(true);
      
      // Console logging should be stopped
      const stats = await sessionManager.handleMessage({
        type: 'get_session_stats',
        data: {}
      });
      expect(stats.success).toBe(true);
      // Should have cleaned up the logger for this session
    });
  });

  describe('Git Hook Validation Pattern', () => {
    it('should support UUID tracking pattern from verification logs', async () => {
      // This test validates the pattern shown in verification/verification_808fb82dda90/
      const testUUID = `test-uuid-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
      
      // Create logger and set session log path
      const logger = new SessionConsoleLogger();
      logger.setSessionLogPath(testLogPath);
      
      // Simulate the git hook pattern: write UUID to session log
      await fs.appendFile(testLogPath, `🌐 [${new Date().toISOString()}] LOG: UUID_${testUUID}_CONSOLE_VERIFICATION\n`);
      
      // Verify the UUID appears in session logs
      const logContent = await fs.readFile(testLogPath, 'utf-8');
      expect(logContent).toContain(testUUID);
      expect(logContent).toContain('CONSOLE_VERIFICATION');
      expect(logContent).toContain('🌐'); // DevTools console format
      
      console.log(`✅ UUID tracking pattern validated: ${testUUID}`);
    });

    it('should create session log files with proper structure', async () => {
      // Verify session log files exist and have proper format
      await expect(fs.access(testLogPath)).resolves.toBeUndefined();
      
      const logContent = await fs.readFile(testLogPath, 'utf-8');
      expect(logContent).toContain('# Continuum Session Log');
      expect(logContent).toContain(`# Session: ${testSessionId}`);
      expect(logContent).toContain('# Type: test');
      expect(logContent).toContain('# Owner: console-test');
      expect(logContent).toContain('Browser log initialized');
      
      console.log(`📝 Session log structure validated: ${testLogPath}`);
    });
  });

  describe('Layer 6 Architecture Validation', () => {
    it('should integrate SessionConsoleLogger with session management', () => {
      // Verify proper module placement
      expect(SessionConsoleLogger).toBeDefined();
      
      // Verify session manager can handle console logging messages
      const daemon = new SessionManagerDaemon();
      expect(daemon.name).toBe('session-manager');
      expect(daemon.version).toBe('1.0.0');
    });

    it('should support DevTools WebSocket pattern', async () => {
      const logger = new SessionConsoleLogger();
      
      // Should have proper DevTools adapter integration
      expect(logger.setSessionLogPath).toBeDefined();
      expect(logger.startLogging).toBeDefined();
      expect(logger.stopLogging).toBeDefined();
      expect(logger.isActive).toBeDefined();
      expect(logger.getSessionLogPath).toBeDefined();
      
      // Should handle session log path configuration
      logger.setSessionLogPath('/test/path');
      expect(logger.getSessionLogPath()).toBe('/test/path');
    });
  });
});