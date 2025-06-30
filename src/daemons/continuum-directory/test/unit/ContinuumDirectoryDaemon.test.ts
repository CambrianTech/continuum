/**
 * Unit Tests for ContinuumDirectoryDaemon
 * Tests isolated daemon functionality without external dependencies
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { ContinuumDirectoryDaemon } from '../../ContinuumDirectoryDaemon.js';
import * as fs from 'fs/promises';
import * as path from 'path';
import * as os from 'os';

describe('ContinuumDirectoryDaemon Unit Tests', () => {
  let daemon: ContinuumDirectoryDaemon;
  let testRoot: string;

  before(async () => {
    // Create temporary test directory
    testRoot = path.join(os.tmpdir(), `continuum-test-${Date.now()}`);
    await fs.mkdir(testRoot, { recursive: true });
    
    daemon = new ContinuumDirectoryDaemon(testRoot);
  });

  after(async () => {
    await daemon.stop();
    // Clean up test directory
    await fs.rm(testRoot, { recursive: true, force: true });
  });

  describe('Daemon Lifecycle', () => {
    it('should start successfully', async () => {
      await daemon.start();
      assert.strictEqual(daemon.isRunning(), true);
    });

    it('should create base directory structure on start', async () => {
      const sessions = path.join(testRoot, 'sessions');
      const config = path.join(testRoot, 'config');
      const logs = path.join(testRoot, 'logs');
      
      const [sessionsExists, configExists, logsExists] = await Promise.all([
        fs.access(sessions).then(() => true).catch(() => false),
        fs.access(config).then(() => true).catch(() => false),
        fs.access(logs).then(() => true).catch(() => false)
      ]);

      assert.strictEqual(sessionsExists, true, 'Sessions directory should exist');
      assert.strictEqual(configExists, true, 'Config directory should exist');
      assert.strictEqual(logsExists, true, 'Logs directory should exist');
    });

    it('should create directory metadata file', async () => {
      const metadataPath = path.join(testRoot, 'directory.json');
      const exists = await fs.access(metadataPath).then(() => true).catch(() => false);
      assert.strictEqual(exists, true, 'Directory metadata should exist');

      const metadata = JSON.parse(await fs.readFile(metadataPath, 'utf-8'));
      assert.strictEqual(metadata.version, '1.0.0');
      assert(Array.isArray(metadata.structure));
    });
  });

  describe('Directory Request Handling', () => {
    it('should handle get_directory message for session type', async () => {
      const message = {
        id: 'test-1',
        type: 'get_directory',
        from: 'test',
        timestamp: new Date(),
        data: {
          type: 'session',
          context: {
            sessionType: 'portal',
            owner: 'test-user',
            sessionId: 'test-session-123'
          }
        }
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, true);
      assert(response.data);
      assert.strictEqual(typeof response.data.path, 'string');
      assert.strictEqual(typeof response.data.created, 'boolean');
      assert(Array.isArray(response.data.structure));
    });

    it('should handle create_session_directory message', async () => {
      const message = {
        id: 'test-2',
        type: 'create_session_directory',
        from: 'test',
        timestamp: new Date(),
        data: {
          sessionType: 'portal',
          owner: 'test-user',
          sessionId: 'test-session-456'
        }
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, true);
      assert(response.data);
      assert.strictEqual(typeof response.data.sessionPath, 'string');
      assert(response.data.artifactPaths);
      
      // Verify session directory was created
      const sessionExists = await fs.access(response.data.sessionPath).then(() => true).catch(() => false);
      assert.strictEqual(sessionExists, true, 'Session directory should be created');
    });

    it('should handle get_artifact_location message', async () => {
      // First create a session
      await daemon['handleMessage']({
        id: 'setup',
        type: 'create_session_directory',
        from: 'test',
        timestamp: new Date(),
        data: {
          sessionType: 'user',
          owner: 'test-user',
          sessionId: 'artifact-test-session'
        }
      });

      const message = {
        id: 'test-3',
        type: 'get_artifact_location',
        from: 'test',
        timestamp: new Date(),
        data: {
          sessionId: 'artifact-test-session',
          artifactType: 'screenshot',
          filename: 'test-screenshot.png'
        }
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, true);
      assert(response.data);
      assert.strictEqual(typeof response.data.path, 'string');
      assert(response.data.path.includes('screenshots'));
      assert(response.data.path.includes('test-screenshot.png'));
    });

    it('should handle get_directory_stats message', async () => {
      const message = {
        id: 'test-4',
        type: 'get_directory_stats',
        from: 'test',
        timestamp: new Date(),
        data: {}
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, true);
      assert(response.data);
      assert.strictEqual(typeof response.data.totalSize, 'number');
      assert.strictEqual(typeof response.data.sessionCount, 'number');
      assert.strictEqual(typeof response.data.healthStatus, 'string');
    });

    it('should handle unknown message type gracefully', async () => {
      const message = {
        id: 'test-5',
        type: 'unknown_message_type',
        from: 'test',
        timestamp: new Date(),
        data: {}
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, false);
      assert(response.error);
      assert(response.error.includes('Unknown message type'));
    });
  });

  describe('Session Directory Management', () => {
    it('should create portal session directory with correct structure', async () => {
      const sessionPath = await daemon['createSessionDirectory'](
        'portal',
        'test-user',
        'portal-session-123'
      );

      assert(sessionPath.includes('portal'));
      assert(sessionPath.includes('portal-session-123'));

      // Check subdirectories exist
      const subdirs = ['artifacts', 'logs', 'screenshots', 'recordings', 'files', 'devtools', 'metadata'];
      for (const subdir of subdirs) {
        const subdirPath = path.join(sessionPath, subdir);
        const exists = await fs.access(subdirPath).then(() => true).catch(() => false);
        assert.strictEqual(exists, true, `Subdirectory ${subdir} should exist`);
      }

      // Check session metadata file
      const metadataPath = path.join(sessionPath, 'session.json');
      const exists = await fs.access(metadataPath).then(() => true).catch(() => false);
      assert.strictEqual(exists, true, 'Session metadata should exist');

      const metadata = JSON.parse(await fs.readFile(metadataPath, 'utf-8'));
      assert.strictEqual(metadata.sessionId, 'portal-session-123');
      assert.strictEqual(metadata.sessionType, 'portal');
      assert.strictEqual(metadata.owner, 'test-user');
    });

    it('should create validation session directory with branch context', async () => {
      const sessionPath = await daemon['createSessionDirectory'](
        'validation',
        'git-hook',
        'validation-session-456',
        { branch: 'feature/test-branch' }
      );

      assert(sessionPath.includes('validation'));
      assert(sessionPath.includes('validation-session-456-feature/test-branch'));
    });

    it('should create persona session directory with owner hierarchy', async () => {
      const sessionPath = await daemon['createSessionDirectory'](
        'persona',
        'academy-student',
        'persona-session-789'
      );

      assert(sessionPath.includes('personas'));
      assert(sessionPath.includes('academy-student'));
      assert(sessionPath.includes('persona-session-789'));
    });
  });

  describe('Artifact Management', () => {
    let testSessionPath: string;

    before(async () => {
      testSessionPath = await daemon['createSessionDirectory'](
        'user',
        'test-user',
        'artifact-management-test'
      );
    });

    it('should generate intelligent artifact location for screenshot', async () => {
      const location = await daemon['getArtifactLocation'](
        'artifact-management-test',
        'screenshot',
        undefined,
        { source: 'browser' }
      );

      assert.strictEqual(typeof location.path, 'string');
      assert(location.path.includes('screenshots'));
      assert(location.path.includes('browser'));
      assert(location.path.includes('screenshot'));
      assert(location.path.endsWith('.png'));
    });

    it('should generate intelligent artifact location for log', async () => {
      const location = await daemon['getArtifactLocation'](
        'artifact-management-test',
        'log',
        'custom-log.log'
      );

      assert(location.path.includes('logs'));
      assert(location.path.includes('custom-log.log'));
    });

    it('should handle devtools artifacts', async () => {
      const location = await daemon['getArtifactLocation'](
        'artifact-management-test',
        'devtools',
        undefined,
        { source: 'console' }
      );

      assert(location.path.includes('devtools'));
      assert(location.path.includes('console'));
      assert(location.path.endsWith('.json'));
    });
  });

  describe('Error Handling', () => {
    it('should handle invalid session creation parameters', async () => {
      const message = {
        id: 'error-test-1',
        type: 'create_session_directory',
        from: 'test',
        timestamp: new Date(),
        data: {
          // Missing required fields
          sessionType: 'portal'
        }
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, false);
      assert(response.error);
      assert(response.error.includes('required'));
    });

    it('should handle artifact location for non-existent session', async () => {
      const message = {
        id: 'error-test-2',
        type: 'get_artifact_location',
        from: 'test',
        timestamp: new Date(),
        data: {
          sessionId: 'non-existent-session',
          artifactType: 'screenshot'
        }
      };

      const response = await daemon['handleMessage'](message);
      
      assert.strictEqual(response.success, false);
      assert(response.error);
      assert(response.error.includes('not found'));
    });
  });
});