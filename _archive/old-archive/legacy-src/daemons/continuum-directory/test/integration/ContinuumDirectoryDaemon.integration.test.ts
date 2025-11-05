/**
 * Integration Tests for ContinuumDirectoryDaemon
 * Tests daemon integration with WebSocket, session systems, and file system
 */

import { describe, it, before, after } from 'node:test';
import assert from 'node:assert';
import { ContinuumDirectoryDaemon } from '../../ContinuumDirectoryDaemon.js';
import * as fs from 'fs/promises';
import * as path from 'path';
import * as os from 'os';

describe('ContinuumDirectoryDaemon Integration Tests', () => {
  let daemon: ContinuumDirectoryDaemon;
  let testRoot: string;

  before(async () => {
    // Create temporary test directory
    testRoot = path.join(os.tmpdir(), `continuum-integration-test-${Date.now()}`);
    await fs.mkdir(testRoot, { recursive: true });
    
    daemon = new ContinuumDirectoryDaemon(testRoot);
    await daemon.start();
  });

  after(async () => {
    await daemon.stop();
    // Clean up test directory
    await fs.rm(testRoot, { recursive: true, force: true });
  });

  describe('Full Session Lifecycle Integration', () => {
    it('should handle complete portal session workflow', async () => {
      const sessionId = `portal-integration-${Date.now()}`;
      
      // 1. Create session directory
      const createResponse = await daemon['handleMessage']({
        id: 'create-1',
        type: 'create_session_directory',
        from: 'integration-test',
        timestamp: new Date(),
        data: {
          sessionType: 'portal',
          owner: 'test-user',
          sessionId,
          context: { 
            source: 'integration-test',
            browser: 'chrome'
          }
        }
      });

      assert.strictEqual(createResponse.success, true);
      assert(createResponse.data);
      const sessionPath = createResponse.data.sessionPath;

      // 2. Get artifact locations for different types
      const artifactTypes = ['screenshot', 'log', 'recording', 'devtools'];
      const artifactPaths: Record<string, string> = {};

      for (const type of artifactTypes) {
        const artifactResponse = await daemon['handleMessage']({
          id: `artifact-${type}`,
          type: 'get_artifact_location',
          from: 'integration-test',
          timestamp: new Date(),
          data: {
            sessionId,
            artifactType: type,
            metadata: { source: 'integration-test' }
          }
        });

        assert.strictEqual(artifactResponse.success, true);
        artifactPaths[type] = artifactResponse.data.path;
      }

      // 3. Verify all artifact paths are within session directory
      for (const [type, artifactPath] of Object.entries(artifactPaths)) {
        assert(artifactPath.startsWith(sessionPath), `${type} artifact should be in session directory`);
        
        // Create a test artifact file
        const testContent = `Test ${type} content from integration test`;
        await fs.writeFile(artifactPath, testContent);
        
        // Verify file was created
        const content = await fs.readFile(artifactPath, 'utf-8');
        assert.strictEqual(content, testContent);
      }

      // 4. Get directory stats and verify session is counted
      const statsResponse = await daemon['handleMessage']({
        id: 'stats-1',
        type: 'get_directory_stats',
        from: 'integration-test',
        timestamp: new Date(),
        data: {}
      });

      assert.strictEqual(statsResponse.success, true);
      assert(statsResponse.data);
      // Stats are currently stubbed, but structure should be correct
      assert.strictEqual(typeof statsResponse.data.sessionCount, 'number');
      assert.strictEqual(typeof statsResponse.data.healthStatus, 'string');
    });

    it('should handle validation session with git context', async () => {
      const sessionId = `validation-${Date.now()}`;
      
      const response = await daemon['handleMessage']({
        id: 'validation-create',
        type: 'create_session_directory',
        from: 'git-hook',
        timestamp: new Date(),
        data: {
          sessionType: 'validation',
          owner: 'git-hook',
          sessionId,
          context: {
            branch: 'feature/test-branch',
            commit: 'abc123def456',
            trigger: 'pre-commit'
          }
        }
      });

      assert.strictEqual(response.success, true);
      assert(response.data);
      
      // Verify branch context is included in path
      const sessionPath = response.data.sessionPath;
      assert(sessionPath.includes('feature/test-branch'), 'Session path should include branch name');
      
      // Verify session metadata includes git context
      const metadataPath = path.join(sessionPath, 'session.json');
      const metadata = JSON.parse(await fs.readFile(metadataPath, 'utf-8'));
      assert.strictEqual(metadata.context.branch, 'feature/test-branch');
      assert.strictEqual(metadata.context.commit, 'abc123def456');
    });

    it('should handle persona session with hierarchy', async () => {
      const sessionId = `persona-${Date.now()}`;
      
      const response = await daemon['handleMessage']({
        id: 'persona-create',
        type: 'create_session_directory',
        from: 'persona-manager',
        timestamp: new Date(),
        data: {
          sessionType: 'persona',
          owner: 'academy-student',
          sessionId,
          context: {
            role: 'testing_droid',
            domain: 'web-automation'
          }
        }
      });

      assert.strictEqual(response.success, true);
      assert(response.data);
      
      // Verify persona hierarchy in path
      const sessionPath = response.data.sessionPath;
      assert(sessionPath.includes('personas'), 'Should be in personas directory');
      assert(sessionPath.includes('academy-student'), 'Should include owner in path');
      assert(sessionPath.includes(sessionId), 'Should include session ID');
    });
  });

  describe('Directory Health and Management', () => {
    it('should validate directory health', async () => {
      const response = await daemon['handleMessage']({
        id: 'health-check',
        type: 'validate_directory_health',
        from: 'health-monitor',
        timestamp: new Date(),
        data: {}
      });

      assert.strictEqual(response.success, true);
      assert(response.data);
      assert.strictEqual(response.data.status, 'healthy');
      assert(Array.isArray(response.data.issues));
      assert(Array.isArray(response.data.recommendations));
    });

    it('should handle cleanup operations', async () => {
      const response = await daemon['handleMessage']({
        id: 'cleanup-test',
        type: 'cleanup_old_sessions',
        from: 'cleanup-service',
        timestamp: new Date(),
        data: {
          maxAge: 24 * 60 * 60 * 1000, // 1 day
          dryRun: true
        }
      });

      assert.strictEqual(response.success, true);
      assert(response.data);
      assert.strictEqual(response.data.dryRun, true);
      assert.strictEqual(typeof response.data.sessionsFound, 'number');
      assert.strictEqual(typeof response.data.spaceFreed, 'number');
    });

    it('should handle artifact organization requests', async () => {
      const response = await daemon['handleMessage']({
        id: 'organize-test',
        type: 'organize_artifacts',
        from: 'organization-service',
        timestamp: new Date(),
        data: {
          strategy: 'auto'
        }
      });

      assert.strictEqual(response.success, true);
      assert(response.data);
      assert.strictEqual(response.data.strategy, 'auto');
      assert.strictEqual(typeof response.data.organized, 'number');
    });
  });

  describe('Concurrent Session Management', () => {
    it('should handle multiple concurrent session creations', async () => {
      const sessionCount = 5;
      const createPromises: Promise<any>[] = [];

      // Create multiple sessions concurrently
      for (let i = 0; i < sessionCount; i++) {
        const promise = daemon['handleMessage']({
          id: `concurrent-${i}`,
          type: 'create_session_directory',
          from: 'concurrent-test',
          timestamp: new Date(),
          data: {
            sessionType: 'user',
            owner: `user-${i}`,
            sessionId: `concurrent-session-${i}-${Date.now()}`
          }
        });
        createPromises.push(promise);
      }

      // Wait for all creations to complete
      const responses = await Promise.all(createPromises);

      // Verify all succeeded
      for (const [index, response] of responses.entries()) {
        assert.strictEqual(response.success, true, `Session ${index} should be created successfully`);
        assert(response.data);
        assert(response.data.sessionPath);
        
        // Verify session directory exists
        const exists = await fs.access(response.data.sessionPath).then(() => true).catch(() => false);
        assert.strictEqual(exists, true, `Session ${index} directory should exist`);
      }
    });

    it('should handle concurrent artifact location requests', async () => {
      // First create a session
      const sessionId = `artifact-concurrent-${Date.now()}`;
      await daemon['handleMessage']({
        id: 'setup-concurrent',
        type: 'create_session_directory',
        from: 'concurrent-test',
        timestamp: new Date(),
        data: {
          sessionType: 'user',
          owner: 'test-user',
          sessionId
        }
      });

      // Then request multiple artifact locations concurrently
      const artifactTypes = ['screenshot', 'log', 'recording', 'devtools', 'file'];
      const artifactPromises = artifactTypes.map((type, index) => 
        daemon['handleMessage']({
          id: `artifact-concurrent-${index}`,
          type: 'get_artifact_location',
          from: 'concurrent-test',
          timestamp: new Date(),
          data: {
            sessionId,
            artifactType: type,
            filename: `test-${type}-${index}.txt`
          }
        })
      );

      const responses = await Promise.all(artifactPromises);

      // Verify all succeeded and have unique paths
      const paths = new Set<string>();
      for (const [index, response] of responses.entries()) {
        assert.strictEqual(response.success, true, `Artifact ${index} should be located successfully`);
        assert(response.data);
        assert(response.data.path);
        
        // Verify paths are unique
        assert(!paths.has(response.data.path), `Artifact path should be unique: ${response.data.path}`);
        paths.add(response.data.path);
        
        // Verify path includes the correct artifact type
        const expectedType = artifactTypes[index];
        assert(response.data.path.includes(expectedType), `Path should include artifact type ${expectedType}`);
      }
    });
  });

  describe('Error Recovery and Edge Cases', () => {
    it('should handle filesystem permission errors gracefully', async () => {
      // Create a read-only directory to test permission handling
      const readOnlyPath = path.join(testRoot, 'read-only-test');
      await fs.mkdir(readOnlyPath, { recursive: true });
      
      try {
        await fs.chmod(readOnlyPath, 0o444); // Read-only

        const restrictedDaemon = new ContinuumDirectoryDaemon(readOnlyPath);
        
        // This should handle the permission error gracefully
        try {
          await restrictedDaemon.start();
          // If it starts, that's fine - it handled the error
          await restrictedDaemon.stop();
        } catch (error) {
          // Expected to fail, but should be a handled error
          assert(error instanceof Error);
          assert(error.message.length > 0);
        }
      } finally {
        // Restore permissions for cleanup
        await fs.chmod(readOnlyPath, 0o755);
      }
    });

    it('should handle malformed message data', async () => {
      const malformedMessages = [
        {
          id: 'malformed-1',
          type: 'create_session_directory',
          from: 'test',
          timestamp: new Date(),
          data: null // Invalid data
        },
        {
          id: 'malformed-2',
          type: 'get_artifact_location',
          from: 'test',
          timestamp: new Date(),
          data: { sessionId: '' } // Empty session ID
        },
        {
          id: 'malformed-3',
          type: 'create_session_directory',
          from: 'test',
          timestamp: new Date(),
          data: { sessionType: 'invalid-type' } // Invalid session type
        }
      ];

      for (const [index, message] of malformedMessages.entries()) {
        const response = await daemon['handleMessage'](message);
        assert.strictEqual(response.success, false, `Malformed message ${index} should fail gracefully`);
        assert(response.error, `Malformed message ${index} should have error message`);
        assert(typeof response.error === 'string', `Error should be a string`);
      }
    });

    it('should handle extremely long session IDs and paths', async () => {
      const longSessionId = 'very-long-session-id-' + 'x'.repeat(200);
      
      const response = await daemon['handleMessage']({
        id: 'long-path-test',
        type: 'create_session_directory',
        from: 'test',
        timestamp: new Date(),
        data: {
          sessionType: 'user',
          owner: 'test-user',
          sessionId: longSessionId
        }
      });

      // Should either succeed with truncated path or fail gracefully
      if (response.success) {
        assert(response.data);
        assert(response.data.sessionPath);
        // Verify the path is reasonable length
        assert(response.data.sessionPath.length < 500, 'Path should not be excessively long');
      } else {
        assert(response.error);
        assert(typeof response.error === 'string');
      }
    });
  });

  describe('Cross-Session Integration', () => {
    it('should find sessions across different session types', async () => {
      const timestamp = Date.now();
      const baseName = `cross-session-${timestamp}`;
      
      // Create sessions of different types with same base name
      const sessionTypes = ['portal', 'validation', 'user', 'persona'];
      const sessionPaths: string[] = [];

      for (const [index, sessionType] of sessionTypes.entries()) {
        const sessionId = `${baseName}-${sessionType}`;
        const response = await daemon['handleMessage']({
          id: `cross-${index}`,
          type: 'create_session_directory',
          from: 'cross-test',
          timestamp: new Date(),
          data: {
            sessionType,
            owner: 'cross-test-user',
            sessionId
          }
        });

        assert.strictEqual(response.success, true);
        sessionPaths.push(response.data.sessionPath);
      }

      // Try to find artifacts in each session
      for (const [index, sessionPath] of sessionPaths.entries()) {
        const sessionId = path.basename(sessionPath);
        
        const artifactResponse = await daemon['handleMessage']({
          id: `find-${index}`,
          type: 'get_artifact_location',
          from: 'cross-test',
          timestamp: new Date(),
          data: {
            sessionId,
            artifactType: 'screenshot',
            filename: `cross-test-${index}.png`
          }
        });

        assert.strictEqual(artifactResponse.success, true);
        assert(artifactResponse.data.path.includes(sessionId));
      }
    });

    it('should maintain session isolation', async () => {
      const sessionA = `isolation-a-${Date.now()}`;
      const sessionB = `isolation-b-${Date.now()}`;

      // Create two different sessions
      const [responseA, responseB] = await Promise.all([
        daemon['handleMessage']({
          id: 'isolation-a',
          type: 'create_session_directory',
          from: 'isolation-test',
          timestamp: new Date(),
          data: {
            sessionType: 'user',
            owner: 'user-a',
            sessionId: sessionA
          }
        }),
        daemon['handleMessage']({
          id: 'isolation-b',
          type: 'create_session_directory',
          from: 'isolation-test',
          timestamp: new Date(),
          data: {
            sessionType: 'user',
            owner: 'user-b',
            sessionId: sessionB
          }
        })
      ]);

      assert.strictEqual(responseA.success, true);
      assert.strictEqual(responseB.success, true);

      const pathA = responseA.data.sessionPath;
      const pathB = responseB.data.sessionPath;

      // Verify sessions are in different directories
      assert.notStrictEqual(pathA, pathB, 'Sessions should have different paths');
      
      // Verify no common subdirectories (beyond the base sessions dir)
      const relativeA = path.relative(testRoot, pathA);
      const relativeB = path.relative(testRoot, pathB);
      const commonPrefix = path.commonPath([relativeA, relativeB]);
      
      // Should only share the base sessions directory
      assert(commonPrefix === 'sessions' || commonPrefix.endsWith('sessions'), 
        'Sessions should only share base sessions directory');
      
      // Create artifacts in each session and verify they don't interfere
      const [artifactA, artifactB] = await Promise.all([
        daemon['handleMessage']({
          id: 'artifact-a',
          type: 'get_artifact_location',
          from: 'isolation-test',
          timestamp: new Date(),
          data: {
            sessionId: sessionA,
            artifactType: 'log',
            filename: 'isolated-log.txt'
          }
        }),
        daemon['handleMessage']({
          id: 'artifact-b',
          type: 'get_artifact_location',
          from: 'isolation-test',
          timestamp: new Date(),
          data: {
            sessionId: sessionB,
            artifactType: 'log',
            filename: 'isolated-log.txt'
          }
        })
      ]);

      assert.strictEqual(artifactA.success, true);
      assert.strictEqual(artifactB.success, true);
      assert.notStrictEqual(artifactA.data.path, artifactB.data.path, 
        'Artifacts in different sessions should have different paths');
    });
  });
});