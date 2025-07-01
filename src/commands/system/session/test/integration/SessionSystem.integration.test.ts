/**
 * Session System Integration Test
 * Tests the complete session management workflow using SessionManagerDaemon
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { SessionManagerDaemon } from '../../../../../daemons/session-manager/SessionManagerDaemon';
import { SessionCommand } from '../../SessionCommand';
import * as fs from 'fs/promises';
import * as path from 'path';
import * as os from 'os';

describe('Session System Integration', () => {
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

  describe('Basic Session Management', () => {
    it('should create a session via command API', async () => {
      const identity = {
        starter: 'cli',
        name: 'joel',
        type: 'development',
        metadata: {
          project: 'continuum',
          branch: 'main'
        }
      };

      const createResult = await SessionCommand.execute(
        { action: 'create', identity },
        mockContext
      );

      expect(createResult.success).toBe(true);
      expect(createResult.data.sessionId).toMatch(/development-joel-/);
      expect(createResult.data.action).toBe('created');
    });

    it('should list sessions via command API', async () => {
      // First create a session directly via daemon
      const createMessage = {
        id: 'test-create',
        from: 'test',
        to: 'session-manager',
        type: 'create_session',
        timestamp: new Date(),
        data: {
          type: 'development',
          owner: 'joel',
          options: {}
        }
      };

      const createResponse = await sessionManagerDaemon.handleMessage(createMessage);
      expect(createResponse.success).toBe(true);

      // Now list sessions via command
      const listResult = await SessionCommand.execute(
        { action: 'list' },
        mockContext
      );

      expect(listResult.success).toBe(true);
      expect(listResult.data.sessions).toHaveLength(1);
      expect(listResult.data.sessions[0].owner).toBe('joel');
      expect(listResult.data.sessions[0].type).toBe('development');
    });

    it('should get session info via command API', async () => {
      // Create session first
      const createMessage = {
        id: 'test-create',
        from: 'test',
        to: 'session-manager',
        type: 'create_session',
        timestamp: new Date(),
        data: {
          type: 'development',
          owner: 'test-user',
          options: {}
        }
      };

      const createResponse = await sessionManagerDaemon.handleMessage(createMessage);
      const sessionId = createResponse.data.sessionId;

      // Get session info via command
      const infoResult = await SessionCommand.execute(
        { action: 'info', sessionId },
        mockContext
      );

      expect(infoResult.success).toBe(true);
      expect(infoResult.data.session.id).toBe(sessionId);
      expect(infoResult.data.session.owner).toBe('test-user');
      expect(infoResult.data.session.artifacts).toBeDefined();
    });

    it('should close session via command API', async () => {
      // Create session first
      const createMessage = {
        id: 'test-create',
        from: 'test',
        to: 'session-manager',
        type: 'create_session',
        timestamp: new Date(),
        data: {
          type: 'test',
          owner: 'temp-user',
          options: {}
        }
      };

      const createResponse = await sessionManagerDaemon.handleMessage(createMessage);
      const sessionId = createResponse.data.sessionId;

      // Verify session exists
      const beforeCloseResult = await SessionCommand.execute(
        { action: 'info', sessionId },
        mockContext
      );
      expect(beforeCloseResult.success).toBe(true);

      // Close session via command
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

  describe('Session Directory Structure', () => {
    it('should create proper directory structure for sessions', async () => {
      const createMessage = {
        id: 'test-create',
        from: 'test',
        to: 'session-manager',
        type: 'create_session',
        timestamp: new Date(),
        data: {
          type: 'development',
          owner: 'joel',
          options: {}
        }
      };

      const createResponse = await sessionManagerDaemon.handleMessage(createMessage);
      expect(createResponse.success).toBe(true);

      const session = createResponse.data.session;
      const storageDir = session.artifacts.storageDir;

      // Verify directory structure exists
      expect(await fs.access(storageDir).then(() => true).catch(() => false)).toBe(true);
      expect(await fs.access(path.join(storageDir, 'logs')).then(() => true).catch(() => false)).toBe(true);
      expect(await fs.access(path.join(storageDir, 'screenshots')).then(() => true).catch(() => false)).toBe(true);
      expect(await fs.access(path.join(storageDir, 'files')).then(() => true).catch(() => false)).toBe(true);
      expect(await fs.access(path.join(storageDir, 'recordings')).then(() => true).catch(() => false)).toBe(true);
      expect(await fs.access(path.join(storageDir, 'devtools')).then(() => true).catch(() => false)).toBe(true);

      // Verify session metadata file
      const metadataPath = path.join(storageDir, 'session-info.json');
      expect(await fs.access(metadataPath).then(() => true).catch(() => false)).toBe(true);

      const metadata = JSON.parse(await fs.readFile(metadataPath, 'utf-8'));
      expect(metadata.sessionId).toBe(session.id);
      expect(metadata.type).toBe('development');
      expect(metadata.owner).toBe('joel');
    });
  });

  describe('CLI Default Connection Pattern', () => {
    it('should demonstrate CLI user connecting to existing or creating new session', async () => {
      // Register CLI user identity
      const registerMessage = {
        id: 'register-cli',
        from: 'test',
        to: 'session-manager',
        type: 'register_connection_identity',
        timestamp: new Date(),
        data: {
          connectionId: 'cli-connection-456',
          identity: {
            type: 'user',
            name: 'joel',
            sessionContext: 'main-development'
          }
        }
      };

      const registerResponse = await sessionManagerDaemon.handleMessage(registerMessage);
      expect(registerResponse.success).toBe(true);

      // Create session for CLI connection
      const createSessionMessage = {
        id: 'create-for-cli',
        from: 'test',
        to: 'session-manager',
        type: 'create_session_for_connection',
        timestamp: new Date(),
        data: {
          connectionId: 'cli-connection-456',
          options: {
            autoCleanup: false
          }
        }
      };

      const sessionResponse = await sessionManagerDaemon.handleMessage(createSessionMessage);
      expect(sessionResponse.success).toBe(true);
      expect(sessionResponse.data.identity.name).toBe('joel');

      // Verify session was created with proper context
      const sessionId = sessionResponse.data.sessionId;
      expect(sessionId).toMatch(/user-joel-/);
    });
  });

  describe('Portal Connection Pattern', () => {
    it('should demonstrate portal connecting with different identity', async () => {
      // Create CLI session first
      const cliMessage = {
        id: 'cli-session',
        from: 'test', 
        to: 'session-manager',
        type: 'create_session',
        timestamp: new Date(),
        data: {
          type: 'development',
          owner: 'joel'
        }
      };

      const cliResponse = await sessionManagerDaemon.handleMessage(cliMessage);
      expect(cliResponse.success).toBe(true);

      // Portal creates its own session
      const portalMessage = {
        id: 'portal-session',
        from: 'test',
        to: 'session-manager', 
        type: 'create_session',
        timestamp: new Date(),
        data: {
          type: 'portal',
          owner: 'portal-debug'
        }
      };

      const portalResponse = await sessionManagerDaemon.handleMessage(portalMessage);
      expect(portalResponse.success).toBe(true);

      // Both sessions should exist independently
      const listResult = await SessionCommand.execute(
        { action: 'list' },
        mockContext
      );

      expect(listResult.success).toBe(true);
      expect(listResult.data.sessions).toHaveLength(2);
      
      const sessionTypes = listResult.data.sessions.map((s: any) => s.type);
      expect(sessionTypes).toContain('development');
      expect(sessionTypes).toContain('portal');
    });
  });
});