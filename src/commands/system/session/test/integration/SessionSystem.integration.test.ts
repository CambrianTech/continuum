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

    it('should create initial log files with session start content', async () => {
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

      // Verify initial log files exist and contain session start content
      const browserLogPath = path.join(storageDir, 'logs', 'browser.log');
      const serverLogPath = path.join(storageDir, 'logs', 'server.log');

      expect(await fs.access(browserLogPath).then(() => true).catch(() => false)).toBe(true);
      expect(await fs.access(serverLogPath).then(() => true).catch(() => false)).toBe(true);

      // Verify log content contains session initialization
      const browserLogContent = await fs.readFile(browserLogPath, 'utf-8');
      const serverLogContent = await fs.readFile(serverLogPath, 'utf-8');

      expect(browserLogContent).toContain(`Session: ${session.id}`);
      expect(browserLogContent).toContain('Browser log initialized');
      expect(serverLogContent).toContain(`Session: ${session.id}`);
      expect(serverLogContent).toContain('Server log initialized');
    });
  });

  describe('Live Log Integration Validation', () => {
    it('should validate log file paths match actual session artifacts', async () => {
      // Use connection orchestration like the real system
      const connectionResult = await sessionManagerDaemon.handleConnect({
        source: 'integration-test',
        owner: 'test-user',
        sessionPreference: 'new',
        capabilities: ['browser', 'logging'],
        context: 'integration-test'
      });

      expect(connectionResult.success).toBe(true);
      expect(connectionResult.data.logs).toBeDefined();
      expect(connectionResult.data.logs.browser).toBeDefined();
      expect(connectionResult.data.logs.server).toBeDefined();

      // CRITICAL: Verify the log file paths actually exist
      const browserLogPath = connectionResult.data.logs.browser;
      const serverLogPath = connectionResult.data.logs.server;

      expect(await fs.access(browserLogPath).then(() => true).catch(() => false)).toBe(true);
      expect(await fs.access(serverLogPath).then(() => true).catch(() => false)).toBe(true);

      // Verify logs contain initialization content
      const browserContent = await fs.readFile(browserLogPath, 'utf-8');
      const serverContent = await fs.readFile(serverLogPath, 'utf-8');

      expect(browserContent).toContain(connectionResult.data.sessionId);
      expect(serverContent).toContain(connectionResult.data.sessionId);
    });

    it('should validate logs are writable and can be appended to', async () => {
      const connectionResult = await sessionManagerDaemon.handleConnect({
        source: 'integration-test',
        owner: 'test-user',
        sessionPreference: 'new',
        capabilities: ['browser', 'logging'],
        context: 'append-test'
      });

      expect(connectionResult.success).toBe(true);

      const browserLogPath = connectionResult.data.logs.browser;
      const serverLogPath = connectionResult.data.logs.server;

      // Test appending to browser log
      const testBrowserMessage = `[${new Date().toISOString()}] TEST: Browser activity logged\n`;
      await fs.appendFile(browserLogPath, testBrowserMessage);

      // Test appending to server log  
      const testServerMessage = `[${new Date().toISOString()}] TEST: Server activity logged\n`;
      await fs.appendFile(serverLogPath, testServerMessage);

      // Verify content was appended
      const browserContent = await fs.readFile(browserLogPath, 'utf-8');
      const serverContent = await fs.readFile(serverLogPath, 'utf-8');

      expect(browserContent).toContain('TEST: Browser activity logged');
      expect(serverContent).toContain('TEST: Server activity logged');
    });

    it('should validate live daemon logging integration to session files', async () => {
      const connectionResult = await sessionManagerDaemon.handleConnect({
        source: 'integration-test',
        owner: 'test-user',
        sessionPreference: 'new',
        capabilities: ['browser', 'logging'],
        context: 'live-logging-test'
      });

      expect(connectionResult.success).toBe(true);
      const serverLogPath = connectionResult.data.logs.server;

      // Set the session manager daemon to log to the session file
      sessionManagerDaemon.setSessionLogPath(serverLogPath);

      // Generate some live log activity
      sessionManagerDaemon['log']('LIVE TEST: This should appear in session log', 'info');
      sessionManagerDaemon['log']('LIVE TEST: Error message test', 'error');
      sessionManagerDaemon['log']('LIVE TEST: Warning message test', 'warn');

      // Wait a bit for async file writes
      await new Promise(resolve => setTimeout(resolve, 100));

      // Read the session log file and verify live content appears
      const logContent = await fs.readFile(serverLogPath, 'utf-8');
      
      // Verify all the live log messages appear in the session file
      expect(logContent).toContain('LIVE TEST: This should appear in session log');
      expect(logContent).toContain('LIVE TEST: Error message test');
      expect(logContent).toContain('LIVE TEST: Warning message test');
      expect(logContent).toContain('[session-manager:');
      
      // Clean up session logging
      sessionManagerDaemon.clearSessionLogPath();
    });

    it('should validate LIVE streaming updates to session logs during daemon activity', async () => {
      const connectionResult = await sessionManagerDaemon.handleConnect({
        source: 'integration-test',
        owner: 'test-user',
        sessionPreference: 'new',
        capabilities: ['browser', 'logging'],
        context: 'live-streaming-test'
      });

      expect(connectionResult.success).toBe(true);
      const serverLogPath = connectionResult.data.logs.server;

      // Set up session logging
      sessionManagerDaemon.setSessionLogPath(serverLogPath);

      // Read initial log content
      const initialContent = await fs.readFile(serverLogPath, 'utf-8');
      const initialLineCount = initialContent.split('\n').length;

      // Generate ongoing daemon activity by making the session manager do work
      sessionManagerDaemon['log']('STREAMING TEST 1: First activity', 'info');
      
      // Wait for async file write
      await new Promise(resolve => setTimeout(resolve, 50));
      
      // Generate more activity
      sessionManagerDaemon['log']('STREAMING TEST 2: Second activity', 'warn');
      sessionManagerDaemon['log']('STREAMING TEST 3: Third activity', 'error');
      
      // Wait for async file writes
      await new Promise(resolve => setTimeout(resolve, 100));

      // Read updated log content
      const updatedContent = await fs.readFile(serverLogPath, 'utf-8');
      const updatedLineCount = updatedContent.split('\n').length;

      // Verify NEW content was ADDED (not just initial content)
      expect(updatedLineCount).toBeGreaterThan(initialLineCount);
      expect(updatedContent).toContain('STREAMING TEST 1: First activity');
      expect(updatedContent).toContain('STREAMING TEST 2: Second activity');
      expect(updatedContent).toContain('STREAMING TEST 3: Third activity');

      // Verify the new content has proper timestamps and daemon info
      expect(updatedContent).toContain('[session-manager:');
      expect(updatedContent).toContain('INFO:');
      expect(updatedContent).toContain('WARN:');
      expect(updatedContent).toContain('ERROR:');

      // Clean up
      sessionManagerDaemon.clearSessionLogPath();
    });

    it('should capture console.log UUID output in session logs (health check pattern)', async () => {
      // This test validates the actual health check pattern:
      // 1. Generate a UUID 
      // 2. console.log it through the system
      // 3. Verify it appears in session logs
      
      const connectionResult = await sessionManagerDaemon.handleConnect({
        source: 'integration-test',
        owner: 'test-user',
        sessionPreference: 'new',
        capabilities: ['browser', 'logging', 'commands'],
        context: 'health-check-test'
      });

      expect(connectionResult.success).toBe(true);
      const serverLogPath = connectionResult.data.logs.server;

      // Set up session logging
      sessionManagerDaemon.setSessionLogPath(serverLogPath);

      // Read initial log content to establish baseline
      await new Promise(resolve => setTimeout(resolve, 50));
      const beforeContent = await fs.readFile(serverLogPath, 'utf-8');

      // Generate a unique UUID for this test (like health checks do)
      const testUUID = `health-check-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
      
      // Simulate the health check pattern: console.log a UUID through the daemon system
      sessionManagerDaemon['log'](`🏥 HEALTH CHECK UUID: ${testUUID}`, 'info');
      sessionManagerDaemon['log'](`🔍 Verifying system responsiveness with marker: ${testUUID}`, 'info');
      sessionManagerDaemon['log'](`✅ Health check ${testUUID} completed successfully`, 'info');

      // Wait for async file writes
      await new Promise(resolve => setTimeout(resolve, 100));

      // Read updated content
      const afterContent = await fs.readFile(serverLogPath, 'utf-8');

      // Verify the UUID appears in the session logs (this is the key test!)
      expect(afterContent).toContain(testUUID);
      expect(afterContent).toContain(`🏥 HEALTH CHECK UUID: ${testUUID}`);
      expect(afterContent).toContain(`🔍 Verifying system responsiveness with marker: ${testUUID}`);
      expect(afterContent).toContain(`✅ Health check ${testUUID} completed successfully`);

      // Verify this is NEW content (not just initial content)
      expect(beforeContent).not.toContain(testUUID);

      // Verify proper daemon log format with timestamps
      const newContent = afterContent.substring(beforeContent.length);
      expect(newContent).toContain('[session-manager:');
      expect(newContent).toMatch(/\[\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z\]/); // timestamp regex

      // Clean up
      sessionManagerDaemon.clearSessionLogPath();
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