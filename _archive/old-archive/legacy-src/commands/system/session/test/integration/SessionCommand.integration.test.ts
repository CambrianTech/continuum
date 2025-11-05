/**
 * SessionCommand Integration Tests
 * Tests actual session management workflows
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { SessionManagerDaemon } from '../../../../../daemons/session-manager/SessionManagerDaemon';
import { SessionCommand } from '../../SessionCommand';
import * as fs from 'fs/promises';
import * as path from 'path';
import * as os from 'os';

describe('SessionCommand Integration', () => {
  let sessionManagerDaemon: SessionManagerDaemon;
  let tempDir: string;
  let mockContext: any;

  beforeEach(async () => {
    // Create temporary directory for test sessions
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'continuum-session-test-'));
    
    // Initialize SessionManagerDaemon with temp directory
    sessionManagerDaemon = new SessionManagerDaemon(tempDir);
    await sessionManagerDaemon.start();
    
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

  describe('CLI User Workflows', () => {
    it('should connect to existing session or create new one', async () => {
      // Register CLI user identity
      const cliIdentity = {
        starter: 'cli' as const,
        identity: {
          starter: 'cli' as const,
          name: 'joel',
          user: 'joel',
          type: 'development' as const,
          metadata: {
            project: 'continuum',
            branch: 'main',
            task: 'testing'
          }
        }
      };

      sessionManager.registerConnectionIdentity('test-connection-123', cliIdentity);

      // First call should create new session since none exists
      const firstResult = await SessionCommand.execute(
        { action: 'current' },
        mockContext
      );

      // Should fail since no session exists yet
      expect(firstResult.success).toBe(false);
      expect(firstResult.error).toBe('No current session found');

      // Create a session using createOrConnectDefaultSession
      const sessionId = await sessionManager.createOrConnectDefaultSession(cliIdentity.identity);
      
      // Now current should work
      const currentResult = await SessionCommand.execute(
        { action: 'current' },
        mockContext
      );

      expect(currentResult.success).toBe(true);
      expect(currentResult.data.session.id).toBe(sessionId);
      expect(currentResult.data.session.identity.name).toBe('joel');
      expect(currentResult.data.session.identity.starter).toBe('cli');

      // Second CLI connection should connect to existing session
      const secondSessionId = await sessionManager.createOrConnectDefaultSession(cliIdentity.identity);
      expect(secondSessionId).toBe(sessionId); // Same session
    });

    it('should create separate sessions for different users', async () => {
      // First user
      const joelIdentity = {
        starter: 'cli' as const,
        name: 'joel',
        user: 'joel',
        type: 'development' as const
      };

      // Second user
      const aliceIdentity = {
        starter: 'cli' as const,
        name: 'alice',
        user: 'alice', 
        type: 'development' as const
      };

      const joelSessionId = await sessionManager.createOrConnectDefaultSession(joelIdentity);
      const aliceSessionId = await sessionManager.createOrConnectDefaultSession(aliceIdentity);

      expect(joelSessionId).not.toBe(aliceSessionId);

      // List sessions should show both
      const listResult = await SessionCommand.execute(
        { action: 'list' },
        mockContext
      );

      expect(listResult.success).toBe(true);
      expect(listResult.data.sessions).toHaveLength(2);
      
      const sessionNames = listResult.data.sessions.map((s: any) => s.identity.name);
      expect(sessionNames).toContain('joel');
      expect(sessionNames).toContain('alice');
    });
  });

  describe('Portal User Workflows', () => {
    it('should allow portal to join existing CLI session', async () => {
      // Create CLI session first
      const cliIdentity = {
        starter: 'cli' as const,
        name: 'joel',
        user: 'joel',
        type: 'development' as const
      };

      const cliSessionId = await sessionManager.createOrConnectDefaultSession(cliIdentity);

      // Portal tries to join
      const portalConnectionId = 'portal-connection-456';
      const portalIdentity = {
        starter: 'portal' as const,
        identity: {
          starter: 'portal' as const,
          name: 'portal-joel',
          user: 'joel', // Same user as CLI
          type: 'collaboration' as const
        }
      };

      sessionManager.registerConnectionIdentity(portalConnectionId, portalIdentity);

      // Portal joins CLI session
      const joinResult = await SessionCommand.execute(
        { action: 'join', sessionId: cliSessionId },
        { ...mockContext, connectionId: portalConnectionId }
      );

      expect(joinResult.success).toBe(true);
      expect(joinResult.data.sessionId).toBe(cliSessionId);
      expect(joinResult.data.action).toBe('joined');
    });

    it('should create separate portal session if needed', async () => {
      const portalIdentity = {
        starter: 'portal' as const,
        name: 'portal-debug',
        user: 'developer',
        type: 'debugging' as const,
        metadata: {
          project: 'continuum',
          task: 'widget-debugging'
        }
      };

      const createResult = await SessionCommand.execute(
        { action: 'create', identity: portalIdentity },
        mockContext
      );

      expect(createResult.success).toBe(true);
      expect(createResult.data.action).toBe('created');

      const sessionId = createResult.data.sessionId;
      expect(sessionId).toMatch(/portal-portal-debug-/);
    });
  });

  describe('Session Querying and Discovery', () => {
    it('should find latest session for user', async () => {
      // Create multiple sessions for same user
      const baseIdentity = {
        starter: 'cli' as const,
        name: 'joel',
        user: 'joel',
        type: 'development' as const
      };

      const session1Id = await sessionManager.createSession(baseIdentity);
      
      // Wait a bit to ensure different timestamps
      await new Promise(resolve => setTimeout(resolve, 10));
      
      const session2Id = await sessionManager.createSession({
        ...baseIdentity,
        metadata: { task: 'newer-task' }
      });

      const latestResult = await SessionCommand.execute(
        { action: 'latest', filter: { user: 'joel' } },
        mockContext
      );

      expect(latestResult.success).toBe(true);
      expect(latestResult.data.session.id).toBe(session2Id); // Should be the newer one
    });

    it('should list joinable sessions', async () => {
      // Create collaboration session
      const collabIdentity = {
        starter: 'portal' as const,
        name: 'team-session',
        user: 'joel',
        type: 'collaboration' as const
      };

      const collabSessionId = await sessionManager.createSession(collabIdentity);

      // Create private development session
      const devIdentity = {
        starter: 'cli' as const,
        name: 'joel-private',
        user: 'joel',
        type: 'development' as const
      };

      await sessionManager.createSession(devIdentity);

      const joinableResult = await SessionCommand.execute(
        { action: 'list', filter: { joinable: true } },
        mockContext
      );

      expect(joinableResult.success).toBe(true);
      
      // Only collaboration sessions should be joinable
      const joinableSessions = joinableResult.data.sessions.filter((s: any) => 
        s.identity.type === 'collaboration'
      );
      
      expect(joinableSessions).toHaveLength(1);
      expect(joinableSessions[0].id).toBe(collabSessionId);
    });

    it('should filter sessions by starter type', async () => {
      // Create sessions from different starters
      await sessionManager.createSession({
        starter: 'cli',
        name: 'cli-user',
        type: 'development'
      });

      await sessionManager.createSession({
        starter: 'portal',
        name: 'portal-user', 
        type: 'debugging'
      });

      await sessionManager.createSession({
        starter: 'git-hook',
        name: 'ci-system',
        type: 'automation'
      });

      // Filter for CLI sessions only
      const cliResult = await SessionCommand.execute(
        { action: 'list', filter: { starter: 'cli' } },
        mockContext
      );

      expect(cliResult.success).toBe(true);
      expect(cliResult.data.sessions).toHaveLength(1);
      expect(cliResult.data.sessions[0].identity.starter).toBe('cli');

      // Filter for portal sessions only  
      const portalResult = await SessionCommand.execute(
        { action: 'list', filter: { starter: 'portal' } },
        mockContext
      );

      expect(portalResult.success).toBe(true);
      expect(portalResult.data.sessions).toHaveLength(1);
      expect(portalResult.data.sessions[0].identity.starter).toBe('portal');
    });
  });

  describe('Session Lifecycle Management', () => {
    it('should get detailed session info', async () => {
      const identity = {
        starter: 'cli' as const,
        name: 'test-user',
        type: 'development' as const,
        metadata: {
          project: 'continuum',
          branch: 'feature-session-mgmt'
        }
      };

      const sessionId = await sessionManager.createSession(identity);

      const infoResult = await SessionCommand.execute(
        { action: 'info', sessionId },
        mockContext
      );

      expect(infoResult.success).toBe(true);
      expect(infoResult.data.session.id).toBe(sessionId);
      expect(infoResult.data.session.identity.metadata.project).toBe('continuum');
      expect(infoResult.data.session.artifacts.storageDir).toMatch(/test-user/);
      expect(infoResult.data.session.mainBrowser).toBeDefined();
      expect(infoResult.data.session.devTools).toBeDefined();
    });

    it('should close session and clean up', async () => {
      const identity = {
        starter: 'cli' as const,
        name: 'temp-user',
        type: 'testing' as const
      };

      const sessionId = await sessionManager.createSession(identity);
      
      // Verify session exists
      const beforeCloseResult = await SessionCommand.execute(
        { action: 'info', sessionId },
        mockContext
      );
      expect(beforeCloseResult.success).toBe(true);

      // Close session
      const closeResult = await SessionCommand.execute(
        { action: 'close', sessionId },
        mockContext
      );

      expect(closeResult.success).toBe(true);
      expect(closeResult.data.action).toBe('closed');

      // Verify session no longer exists
      const afterCloseResult = await SessionCommand.execute(
        { action: 'info', sessionId },
        mockContext
      );
      expect(afterCloseResult.success).toBe(false);
      expect(afterCloseResult.error).toMatch(/not found/);
    });
  });

  describe('DevTools Scenarios', () => {
    it('should support sessions with DevTools enabled', async () => {
      const devIdentity = {
        starter: 'portal' as const,
        name: 'debug-session',
        user: 'developer',
        type: 'debugging' as const,
        metadata: {
          project: 'continuum',
          task: 'widget-devtools'
        }
      };

      const sessionId = await sessionManager.createSession(devIdentity);
      
      // Launch browser with DevTools
      const launchResult = await sessionManager.launchBrowserForSession(sessionId, {
        openDevTools: true,
        devToolsTabs: ['console', 'network', 'sources']
      });

      expect(launchResult.success).toBe(true);

      // Check session info includes DevTools state
      const infoResult = await SessionCommand.execute(
        { action: 'info', sessionId },
        mockContext
      );

      expect(infoResult.success).toBe(true);
      expect(infoResult.data.session.devTools.isOpen).toBe(true);
      expect(infoResult.data.session.devTools.tabs).toEqual(['console', 'network', 'sources']);
    });
  });
});