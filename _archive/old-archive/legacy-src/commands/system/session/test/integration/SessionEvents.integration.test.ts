/**
 * Session Events Integration Test
 * Demonstrates the complete event-driven session management workflow
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { SessionManagerDaemon, SessionEvent } from '../../../../../daemons/session-manager/SessionManagerDaemon';
import { SessionCommand } from '../../SessionCommand';
import * as fs from 'fs/promises';
import * as path from 'path';
import * as os from 'os';

describe('Session Events Integration', () => {
  let sessionManagerDaemon: SessionManagerDaemon;
  let tempDir: string;
  let mockContext: any;
  let capturedEvents: SessionEvent[] = [];

  beforeEach(async () => {
    // Create temporary directory for test sessions
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'continuum-session-events-test-'));
    
    // Initialize SessionManagerDaemon with temp directory
    sessionManagerDaemon = new SessionManagerDaemon(tempDir);
    await sessionManagerDaemon.start();
    
    // Set up event capture
    capturedEvents = [];
    sessionManagerDaemon.onSessionEvent((event: SessionEvent) => {
      capturedEvents.push(event);
    });
    
    // Create mock context that includes the session manager daemon
    mockContext = {
      connectionId: 'test-connection-123',
      websocket: {
        registeredDaemons: new Map([
          ['session-manager', sessionManagerDaemon]
        ])
      }
    };
  });

  afterEach(async () => {
    // Cleanup sessions and temp directory
    await sessionManagerDaemon.stop();
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  describe('Event-Driven Session Lifecycle', () => {
    it('should emit events throughout session lifecycle', async () => {
      // 1. Register connection identity - should emit connection_registered
      const registerMessage = {
        id: 'register-test',
        from: 'test',
        to: 'session-manager',
        type: 'register_connection_identity',
        timestamp: new Date(),
        data: {
          connectionId: 'cli-user-456',
          identity: {
            type: 'user',
            name: 'joel',
            sessionContext: 'main-development'
          }
        }
      };

      const registerResponse = await sessionManagerDaemon.handleMessage(registerMessage);
      expect(registerResponse.success).toBe(true);

      // Verify connection_registered event was emitted
      expect(capturedEvents).toHaveLength(1);
      expect(capturedEvents[0].type).toBe('connection_registered');
      expect(capturedEvents[0].connectionId).toBe('cli-user-456');
      expect(capturedEvents[0].identity?.name).toBe('joel');

      // 2. Create session - should emit session_created
      const createMessage = {
        id: 'create-test',
        from: 'test',
        to: 'session-manager',
        type: 'create_session',
        timestamp: new Date(),
        data: {
          type: 'development',
          owner: 'joel',
          options: {
            sessionContext: 'main-development'
          }
        }
      };

      const createResponse = await sessionManagerDaemon.handleMessage(createMessage);
      expect(createResponse.success).toBe(true);

      // Verify session_created event was emitted
      expect(capturedEvents).toHaveLength(2);
      expect(capturedEvents[1].type).toBe('session_created');
      expect(capturedEvents[1].sessionId).toBeDefined();
      expect(capturedEvents[1].session?.owner).toBe('joel');
      expect(capturedEvents[1].metadata?.storageDir).toMatch(/joel/);

      const sessionId = capturedEvents[1].sessionId!;
      const storageDir = capturedEvents[1].metadata?.storageDir;

      // 3. Close session - should emit session_closed
      const closeMessage = {
        id: 'close-test',
        from: 'test',
        to: 'session-manager',
        type: 'close_session',
        timestamp: new Date(),
        data: {
          sessionId,
          preserveArtifacts: true
        }
      };

      const closeResponse = await sessionManagerDaemon.handleMessage(closeMessage);
      expect(closeResponse.success).toBe(true);

      // Verify session_closed event was emitted
      expect(capturedEvents).toHaveLength(3);
      expect(capturedEvents[2].type).toBe('session_closed');
      expect(capturedEvents[2].sessionId).toBe(sessionId);
      expect(capturedEvents[2].metadata?.storageDir).toBe(storageDir);
      expect(capturedEvents[2].metadata?.preserveArtifacts).toBe(true);
    });

    it('should provide complete session info in events for log path generation', async () => {
      // Create session via command API
      const identity = {
        starter: 'cli',
        name: 'joel',
        type: 'development',
        metadata: {
          project: 'continuum',
          branch: 'feature-session-events'
        }
      };

      const createResult = await SessionCommand.execute(
        { action: 'create', identity },
        mockContext
      );

      expect(createResult.success).toBe(true);

      // Verify session_created event contains all necessary info
      const sessionCreatedEvent = capturedEvents.find(e => e.type === 'session_created');
      expect(sessionCreatedEvent).toBeDefined();
      
      const sessionInfo = sessionCreatedEvent!;
      const storageDir = sessionInfo.metadata?.storageDir;
      
      // Generate log paths from event data
      const browserLogPath = `${storageDir}/logs/browser.log`;
      const serverLogPath = `${storageDir}/logs/server.log`;
      const screenshotDir = `${storageDir}/screenshots`;
      
      expect(storageDir).toMatch(/joel/);
      expect(browserLogPath).toMatch(/\.continuum\/sessions.*joel.*logs\/browser\.log/);
      expect(serverLogPath).toMatch(/\.continuum\/sessions.*joel.*logs\/server\.log/);
      expect(screenshotDir).toMatch(/\.continuum\/sessions.*joel.*screenshots/);

      // Verify directories actually exist (created by session manager)
      expect(await fs.access(storageDir!).then(() => true).catch(() => false)).toBe(true);
      expect(await fs.access(path.dirname(browserLogPath)).then(() => true).catch(() => false)).toBe(true);
      expect(await fs.access(screenshotDir).then(() => true).catch(() => false)).toBe(true);
    });

    it('should demonstrate event-driven coordination between daemons', async () => {
      // Simulate how other daemons would coordinate using events
      const logManager = {
        name: 'log-manager',
        sessionLogPaths: new Map<string, { browser: string; server: string }>()
      };

      const screenshotManager = {
        name: 'screenshot-manager', 
        sessionScreenshotDirs: new Map<string, string>()
      };

      // Set up event listeners to coordinate with session manager
      sessionManagerDaemon.onSessionEvent((event: SessionEvent) => {
        if (event.type === 'session_created') {
          const sessionId = event.sessionId!;
          const storageDir = event.metadata?.storageDir!;
          
          // Log manager captures log paths
          logManager.sessionLogPaths.set(sessionId, {
            browser: `${storageDir}/logs/browser.log`,
            server: `${storageDir}/logs/server.log`
          });
          
          // Screenshot manager captures screenshot directory
          screenshotManager.sessionScreenshotDirs.set(sessionId, `${storageDir}/screenshots`);
        }

        if (event.type === 'session_closed') {
          const sessionId = event.sessionId!;
          
          // Clean up tracking when session closes
          logManager.sessionLogPaths.delete(sessionId);
          screenshotManager.sessionScreenshotDirs.delete(sessionId);
        }
      });

      // Create session
      const createMessage = {
        id: 'coordination-test',
        from: 'test',
        to: 'session-manager',
        type: 'create_session',
        timestamp: new Date(),
        data: {
          type: 'development',
          owner: 'coordination-user'
        }
      };

      const createResponse = await sessionManagerDaemon.handleMessage(createMessage);
      const sessionId = createResponse.data.sessionId;

      // Verify coordination worked
      expect(logManager.sessionLogPaths.has(sessionId)).toBe(true);
      expect(screenshotManager.sessionScreenshotDirs.has(sessionId)).toBe(true);

      const logPaths = logManager.sessionLogPaths.get(sessionId)!;
      expect(logPaths.browser).toMatch(/coordination-user.*logs\/browser\.log/);
      expect(logPaths.server).toMatch(/coordination-user.*logs\/server\.log/);

      const screenshotDir = screenshotManager.sessionScreenshotDirs.get(sessionId)!;
      expect(screenshotDir).toMatch(/coordination-user.*screenshots/);

      // Close session
      const closeMessage = {
        id: 'close-coordination-test',
        from: 'test',
        to: 'session-manager',
        type: 'close_session',
        timestamp: new Date(),
        data: { sessionId }
      };

      await sessionManagerDaemon.handleMessage(closeMessage);

      // Verify cleanup coordination worked
      expect(logManager.sessionLogPaths.has(sessionId)).toBe(false);
      expect(screenshotManager.sessionScreenshotDirs.has(sessionId)).toBe(false);
    });

    it('should support querying session info via commands anytime', async () => {
      // Create session
      const createMessage = {
        id: 'query-test',
        from: 'test',
        to: 'session-manager',
        type: 'create_session',
        timestamp: new Date(),
        data: {
          type: 'portal',
          owner: 'debug-user',
          options: {
            sessionContext: 'widget-debugging'
          }
        }
      };

      const createResponse = await sessionManagerDaemon.handleMessage(createMessage);
      const sessionId = createResponse.data.sessionId;

      // Query session info via command (alternative to events)
      const infoResult = await SessionCommand.execute(
        { action: 'info', sessionId },
        mockContext
      );

      expect(infoResult.success).toBe(true);
      expect(infoResult.data.session.id).toBe(sessionId);
      expect(infoResult.data.session.owner).toBe('debug-user');
      expect(infoResult.data.session.artifacts.storageDir).toMatch(/debug-user/);

      // List all sessions via command
      const listResult = await SessionCommand.execute(
        { action: 'list' },
        mockContext
      );

      expect(listResult.success).toBe(true);
      expect(listResult.data.sessions).toHaveLength(1);
      expect(listResult.data.sessions[0].id).toBe(sessionId);

      // Filter sessions via command
      const filterResult = await SessionCommand.execute(
        { action: 'list', filter: { type: 'portal' } },
        mockContext
      );

      expect(filterResult.success).toBe(true);
      expect(filterResult.data.sessions).toHaveLength(1);
      expect(filterResult.data.sessions[0].type).toBe('portal');
    });
  });

  describe('CLI Connection Pattern with Events', () => {
    it('should demonstrate CLI default connection with event capture', async () => {
      let sessionInfo: { sessionId: string; storageDir: string } | null = null;

      // Set up promise to capture session creation
      const sessionPromise = new Promise<{ sessionId: string; storageDir: string }>((resolve) => {
        sessionManagerDaemon.onSessionEvent((event: SessionEvent) => {
          if (event.type === 'session_created') {
            resolve({
              sessionId: event.sessionId!,
              storageDir: event.metadata?.storageDir!
            });
          }
        });
      });

      // Simulate CLI startup process
      // 1. Register CLI connection
      const registerMessage = {
        id: 'cli-register',
        from: 'cli',
        to: 'session-manager',
        type: 'register_connection_identity',
        timestamp: new Date(),
        data: {
          connectionId: 'cli-main-789',
          identity: {
            type: 'user',
            name: 'joel',
            sessionContext: 'main-development'
          }
        }
      };

      await sessionManagerDaemon.handleMessage(registerMessage);

      // 2. Create session for CLI connection
      const createSessionMessage = {
        id: 'cli-create-session',
        from: 'cli',
        to: 'session-manager',
        type: 'create_session_for_connection',
        timestamp: new Date(),
        data: {
          connectionId: 'cli-main-789',
          options: { autoCleanup: false }
        }
      };

      const sessionResponse = await sessionManagerDaemon.handleMessage(createSessionMessage);
      expect(sessionResponse.success).toBe(true);

      // 3. Capture session info from event
      sessionInfo = await sessionPromise;

      // Verify session info can be used for path generation
      expect(sessionInfo.sessionId).toMatch(/user-joel-/);
      expect(sessionInfo.storageDir).toMatch(/joel/);

      const browserLogPath = `${sessionInfo.storageDir}/logs/browser.log`;
      const serverLogPath = `${sessionInfo.storageDir}/logs/server.log`;

      console.log(`📊 Browser logs: ${browserLogPath}`);
      console.log(`📋 Server logs: ${serverLogPath}`);

      // This is exactly what main.ts would display to user
      expect(browserLogPath).toMatch(/\.continuum\/sessions\/user-joel-.+\/logs\/browser\.log/);
      expect(serverLogPath).toMatch(/\.continuum\/sessions\/user-joel-.+\/logs\/server\.log/);
    });
  });
});