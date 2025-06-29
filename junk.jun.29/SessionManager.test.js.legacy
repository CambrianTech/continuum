/**
 * SessionManager Unit Tests
 * Self-contained tests for unified session management system
 */

const fs = require('fs').promises;
const path = require('path');
const { execSync } = require('child_process');
const SessionManager = require('../SessionManager.cjs');

describe('SessionManager', () => {
  let sessionManager;
  let testDir;

  beforeEach(async () => {
    // Create temporary test directory
    testDir = path.join(__dirname, 'temp_test_sessions');
    await fs.mkdir(testDir, { recursive: true });
    sessionManager = new SessionManager(testDir);
  });

  afterEach(async () => {
    // Clean up test directory
    try {
      await fs.rm(testDir, { recursive: true, force: true });
    } catch (e) {
      // Cleanup failure shouldn't break tests
    }
  });

  describe('createSession', () => {
    test('should create session directory with metadata', async () => {
      const runId = 'test123';
      const metadata = { user: 'test', action: 'screenshot' };
      
      const sessionPath = await sessionManager.createSession('portal', runId, metadata);
      
      // Verify directory exists
      const stats = await fs.stat(sessionPath);
      expect(stats.isDirectory()).toBe(true);
      
      // Verify metadata file
      const metadataPath = path.join(sessionPath, 'session.json');
      const metadataContent = await fs.readFile(metadataPath, 'utf8');
      const sessionMeta = JSON.parse(metadataContent);
      
      expect(sessionMeta.sessionType).toBe('portal');
      expect(sessionMeta.runId).toBe(runId);
      expect(sessionMeta.user).toBe('test');
      expect(sessionMeta.action).toBe('screenshot');
    });

    test('should handle nested session types', async () => {
      const sessionPath = await sessionManager.createSession('personas/testing-droid', 'run123');
      expect(sessionPath).toContain('personas/testing-droid/run_run123');
    });
  });

  describe('completeSession', () => {
    test('should update session with completion data and create latest symlink', async () => {
      const runId = 'complete_test';
      
      // Create session first
      await sessionManager.createSession('portal', runId);
      
      // Complete session
      const results = { success: true, summary: 'Test completed' };
      const sessionPath = await sessionManager.completeSession('portal', runId, results);
      
      // Verify completion metadata
      const metadataPath = path.join(sessionPath, 'session.json');
      const metadataContent = await fs.readFile(metadataPath, 'utf8');
      const sessionMeta = JSON.parse(metadataContent);
      
      expect(sessionMeta.status).toBe('PASS');
      expect(sessionMeta.results.success).toBe(true);
      expect(sessionMeta.endTime).toBeDefined();
      
      // Verify latest symlink exists
      const latestPath = path.join(testDir, 'sessions', 'portal', 'latest');
      const stats = await fs.lstat(latestPath);
      expect(stats.isSymbolicLink()).toBe(true);
      
      // Verify symlink points to correct session
      const linkTarget = await fs.readlink(latestPath);
      expect(linkTarget).toBe(`run_${runId}`);
    });

    test('should handle failure status correctly', async () => {
      const runId = 'fail_test';
      
      await sessionManager.createSession('portal', runId);
      
      const results = { success: false, error: 'Test failed' };
      await sessionManager.completeSession('portal', runId, results);
      
      const metadataPath = path.join(testDir, 'sessions', 'portal', `run_${runId}`, 'session.json');
      const metadataContent = await fs.readFile(metadataPath, 'utf8');
      const sessionMeta = JSON.parse(metadataContent);
      
      expect(sessionMeta.status).toBe('FAIL');
    });
  });

  describe('artifact management', () => {
    test('should write and read text artifacts', async () => {
      const runId = 'artifact_test';
      const sessionPath = await sessionManager.createSession('portal', runId);
      
      const testContent = 'Test log content';
      await sessionManager.writeArtifact(sessionPath, 'client-logs', testContent);
      
      const readContent = await sessionManager.readArtifact('portal', runId, 'client-logs');
      expect(readContent).toBe(testContent);
    });

    test('should handle binary artifacts', async () => {
      const runId = 'binary_test';
      const sessionPath = await sessionManager.createSession('portal', runId);
      
      const testBuffer = Buffer.from('fake image data');
      await sessionManager.writeArtifact(sessionPath, 'ui-capture.png', testBuffer);
      
      const readBuffer = await sessionManager.readArtifact('portal', runId, 'ui-capture');
      expect(Buffer.isBuffer(readBuffer)).toBe(true);
      expect(readBuffer.equals(testBuffer)).toBe(true);
    });

    test('should support latest session access', async () => {
      const runId1 = 'old_session';
      const runId2 = 'new_session';
      
      // Create and complete first session
      await sessionManager.createSession('portal', runId1);
      await sessionManager.completeSession('portal', runId1, { success: true });
      await sessionManager.writeArtifact(
        path.join(testDir, 'sessions', 'portal', `run_${runId1}`),
        'client-logs',
        'old content'
      );
      
      // Create and complete second session (becomes latest)
      await sessionManager.createSession('portal', runId2);
      await sessionManager.completeSession('portal', runId2, { success: true });
      await sessionManager.writeArtifact(
        path.join(testDir, 'sessions', 'portal', `run_${runId2}`),
        'client-logs',
        'new content'
      );
      
      // Read from latest should get new content
      const latestContent = await sessionManager.readArtifact('portal', 'latest', 'client-logs');
      expect(latestContent).toBe('new content');
    });
  });

  describe('history management', () => {
    test('should create unified history format', async () => {
      const runId = 'history_test';
      
      await sessionManager.createSession('portal', runId, { action: 'test' });
      await sessionManager.completeSession('portal', runId, { 
        success: true, 
        summary: 'Test session completed successfully' 
      });
      
      const historyPath = path.join(testDir, 'sessions', 'portal', 'history.txt');
      const historyContent = await fs.readFile(historyPath, 'utf8');
      
      expect(historyContent).toContain('✅');
      expect(historyContent).toContain('history_t'); // Truncated runId
      expect(historyContent).toContain('Test session completed successfully');
    });
  });

  describe('executeSessionCommand', () => {
    test('should handle create command', async () => {
      const result = await sessionManager.executeSessionCommand('create', {
        type: 'portal',
        runId: 'cmd_test',
        metadata: { test: true }
      });
      
      expect(result).toContain('sessions/portal/run_cmd_test');
    });

    test('should handle list command', async () => {
      // Create a few sessions
      await sessionManager.createSession('portal', 'list_test1');
      await sessionManager.createSession('portal', 'list_test2');
      
      const result = await sessionManager.executeSessionCommand('list', {
        type: 'portal',
        limit: 10
      });
      
      expect(Array.isArray(result)).toBe(true);
      expect(result.length).toBe(2);
    });

    test('should handle unknown command gracefully', async () => {
      await expect(
        sessionManager.executeSessionCommand('unknown', {})
      ).rejects.toThrow('Unknown session command: unknown');
    });
  });

  describe('static factory method', () => {
    test('should create SessionManager via factory', () => {
      const manager = SessionManager.createForContinuum('/test/path');
      expect(manager).toBeInstanceOf(SessionManager);
      expect(manager.baseDir).toBe('/test/path');
    });
  });
});

module.exports = {
  testSessionManager: () => {
    console.log('✅ SessionManager tests available');
    console.log('Run: npm test -- SessionManager.test.js');
  }
};