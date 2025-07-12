/**
 * Shared Session + Browser Launch Integration Test
 * 
 * Tests the complete end-to-end behavior we just implemented:
 * - One shared session created and reused consistently
 * - Browser launched when needed (with safety checks)
 * - No runaway session/browser creation
 * - Proper session affinity and cleanup
 * 
 * This validates the exact scenario that was broken and is now fixed.
 */

import { describe, it, beforeEach, afterEach } from 'node:test';
import * as assert from 'node:assert';
import { SessionManagerDaemon } from '../../daemons/session-manager/SessionManagerDaemon';
import { BrowserManagerDaemon } from '../../daemons/browser-manager/BrowserManagerDaemon';
import { ConnectCommand } from '../../commands/kernel/connect/ConnectCommand';
import * as fs from 'fs/promises';
import * as path from 'path';
import * as os from 'os';
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

describe('Shared Session + Browser Launch Integration', () => {
  let sessionManager: SessionManagerDaemon;
  let browserManager: BrowserManagerDaemon;
  let tempDir: string;
  let mockContext: any;
  let launchedBrowserPids: number[] = [];

  beforeEach(async () => {
    // Create temporary directory for test sessions
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'continuum-shared-session-test-'));
    
    // Initialize daemons
    sessionManager = new SessionManagerDaemon(tempDir);
    browserManager = new BrowserManagerDaemon();
    
    await sessionManager.start();
    await browserManager.start();
    
    // Wire up session events for browser management (like real system)
    sessionManager.on('session_created', (event: any) => {
      browserManager.emit('session_created', event);
    });
    
    sessionManager.on('session_joined', (event: any) => {
      browserManager.emit('session_joined', event);  
    });
    
    // Create mock context that includes both daemons
    mockContext = {
      connectionId: 'test-connection-shared-session',
      websocket: {
        registeredDaemons: new Map([
          ['session-manager', sessionManager],
          ['browser-manager', browserManager]
        ])
      }
    };
    
    launchedBrowserPids = [];
  });

  afterEach(async () => {
    // Kill any browsers launched during test
    for (const pid of launchedBrowserPids) {
      try {
        await execAsync(`kill -9 ${pid} 2>/dev/null || true`);
      } catch (error) {
        // Expected if process already dead
      }
    }
    
    // Stop daemons
    await browserManager.stop();
    await sessionManager.stop();
    
    // Cleanup temp directory
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  describe('Core Shared Session Behavior', () => {
    it('should create ONE shared session and reuse it consistently', async () => {
      // Test 1: First connect call - should create new session
      const firstResult = await ConnectCommand.executeOperation({}, mockContext);
      
      assert.strictEqual(firstResult.success, true);
      assert.strictEqual(firstResult.data.session.action, 'created_new');
      const firstSessionId = firstResult.data.session.sessionId;
      assert.strictEqual(firstSessionId).toMatch(/development-shared-/);
      
      // Test 2: Second connect call - should reuse same session
      const secondResult = await ConnectCommand.executeOperation({}, mockContext);
      
      assert.strictEqual(secondResult.success, true);
      assert.strictEqual(secondResult.data.session.action, 'joined_existing');
      assert.strictEqual(secondResult.data.session.sessionId, firstSessionId);
      
      // Test 3: Third connect call - should still reuse same session
      const thirdResult = await ConnectCommand.executeOperation({}, mockContext);
      
      assert.strictEqual(thirdResult.success, true);
      assert.strictEqual(thirdResult.data.session.action, 'joined_existing');
      assert.strictEqual(thirdResult.data.session.sessionId, firstSessionId);
      
      // Verify only ONE session directory exists
      const sessionDirs = await fs.readdir(path.join(tempDir, 'user', 'shared'));
      assert.strictEqual(sessionDirs).toHaveLength(1);
      assert.strictEqual(sessionDirs[0], firstSessionId);
    });

    it('should handle sequential connect calls with proper session reuse', async () => {
      // Sequential connects (not simultaneous) should demonstrate proper reuse
      const firstResult = await ConnectCommand.executeOperation({}, mockContext);
      assert.strictEqual(firstResult.success, true);
      assert.strictEqual(firstResult.data.session.action, 'created_new');
      const sessionId = firstResult.data.session.sessionId;
      
      // Subsequent sequential calls should reuse the session
      for (let i = 0; i < 3; i++) {
        const result = await ConnectCommand.executeOperation({}, mockContext);
        assert.strictEqual(result.success, true);
        assert.strictEqual(result.data.session.sessionId, sessionId);
        assert.strictEqual(result.data.session.action, 'joined_existing');
      }
      
      // Verify only ONE session directory exists
      const sessionDirs = await fs.readdir(path.join(tempDir, 'user', 'shared'));
      assert.strictEqual(sessionDirs).toHaveLength(1);
      assert.strictEqual(sessionDirs[0], sessionId);
    });

    it('should create different sessions for different session types', async () => {
      // Default connect (development session)
      const devResult = await ConnectCommand.executeOperation({}, mockContext);
      assert.strictEqual(devResult.success, true);
      const devSessionId = devResult.data.session.sessionId;
      
      // Connect with different session type
      const testResult = await ConnectCommand.executeOperation({
        sessionType: 'test'
      }, mockContext);
      assert.strictEqual(testResult.success, true);
      const testSessionId = testResult.data.session.sessionId;
      
      // Should be different sessions due to different types
      assert.strictEqual(devSessionId).not.toBe(testSessionId);
      assert.strictEqual(devSessionId).toMatch(/development-/);
      assert.strictEqual(testSessionId).toMatch(/test-/);
      
      // But reusing development should still return same dev session
      const devAgainResult = await ConnectCommand.executeOperation({
        sessionType: 'development'
      }, mockContext);
      assert.strictEqual(devAgainResult.data.session.sessionId, devSessionId);
    });
  });

  describe('Browser Launch Safety Integration', () => {
    it('should launch browser for NEW session but not for EXISTING session joins', async () => {
      // Mock browser launch tracking
      let browserLaunchCount = 0;
      const originalLaunch = browserManager['safelyLaunchBrowserForSession'];
      const originalEnsure = browserManager['ensureBrowserExistsForSession'];
      
      browserManager['safelyLaunchBrowserForSession'] = async function(...args) {
        browserLaunchCount++;
        this.log(`🧪 TEST: safelyLaunchBrowserForSession called (count: ${browserLaunchCount})`);
        // Don't actually launch browser in test, just track the call
        return Promise.resolve();
      };
      
      let browserEnsureCount = 0;
      browserManager['ensureBrowserExistsForSession'] = async function(...args) {
        browserEnsureCount++;
        this.log(`🧪 TEST: ensureBrowserExistsForSession called (count: ${browserEnsureCount})`);
        return Promise.resolve();
      };
      
      // First connect - should trigger safe launch for NEW session
      const firstResult = await ConnectCommand.executeOperation({}, mockContext);
      assert.strictEqual(firstResult.success, true);
      assert.strictEqual(firstResult.data.session.action, 'created_new');
      
      // Wait for async browser management
      await new Promise(resolve => setTimeout(resolve, 100));
      assert.strictEqual(browserLaunchCount, 1); // Called once for new session
      
      // Second connect - should trigger ensure for EXISTING session
      const secondResult = await ConnectCommand.executeOperation({}, mockContext);
      assert.strictEqual(secondResult.success, true);
      assert.strictEqual(secondResult.data.session.action, 'joined_existing');
      
      await new Promise(resolve => setTimeout(resolve, 100));
      assert.strictEqual(browserLaunchCount, 1); // Still only called once
      assert.strictEqual(browserEnsureCount, 1); // Called once for existing session
      
      // Third connect - should trigger ensure again
      const thirdResult = await ConnectCommand.executeOperation({}, mockContext);
      assert.strictEqual(thirdResult.success, true);
      assert.strictEqual(thirdResult.data.session.action, 'joined_existing');
      
      await new Promise(resolve => setTimeout(resolve, 100));
      assert.strictEqual(browserLaunchCount, 1); // Still only called once
      assert.strictEqual(browserEnsureCount, 2); // Called twice for existing sessions
      
      // Restore original methods
      browserManager['safelyLaunchBrowserForSession'] = originalLaunch;
      browserManager['ensureBrowserExistsForSession'] = originalEnsure;
    });

    it('should prevent runaway browser creation with multiple safety checks', async () => {
      // Mock tab manager to simulate existing browser
      const originalCheckTabs = browserManager['tabManager']?.checkTabs;
      if (browserManager['tabManager']) {
        browserManager['tabManager'].checkTabs = async () => ({
          count: 1,
          action: 'existing_connection',
          details: [{ pid: 12345, process: 'TestBrowser', state: 'ESTABLISHED' }]
        });
      }
      
      let actualLaunchAttempts = 0;
      const originalLauncher = browserManager['launcher'];
      if (originalLauncher) {
        originalLauncher.launch = async (...args) => {
          actualLaunchAttempts++;
          throw new Error('Should not launch - browser already exists');
        };
      }
      
      // Multiple connect calls should NOT trigger browser launches
      await ConnectCommand.executeOperation({}, mockContext);
      await ConnectCommand.executeOperation({}, mockContext);
      await ConnectCommand.executeOperation({}, mockContext);
      
      await new Promise(resolve => setTimeout(resolve, 200));
      
      // No actual browser launches should occur (safety checks prevent it)
      assert.strictEqual(actualLaunchAttempts, 0);
      
      // Restore original methods
      if (browserManager['tabManager'] && originalCheckTabs) {
        browserManager['tabManager'].checkTabs = originalCheckTabs;
      }
    });
  });

  describe('Session Artifact Management', () => {
    it('should create proper session directory structure for shared session', async () => {
      const result = await ConnectCommand.executeOperation({}, mockContext);
      assert.strictEqual(result.success, true);
      
      const sessionId = result.data.session.sessionId;
      const sessionDir = path.join(tempDir, 'user', 'shared', sessionId);
      
      // Verify session directory exists
      assert.strictEqual(await fs.access(sessionDir).then(() => true).catch(() => false), true);
      
      // Verify subdirectories
      const subdirs = ['logs', 'screenshots', 'files', 'recordings', 'devtools'];
      for (const subdir of subdirs) {
        const subdirPath = path.join(sessionDir, subdir);
        assert.strictEqual(await fs.access(subdirPath).then(() => true).catch(() => false), true);
      }
      
      // Verify log files exist
      const browserLogPath = path.join(sessionDir, 'logs', 'browser.log');
      const serverLogPath = path.join(sessionDir, 'logs', 'server.log');
      
      assert.strictEqual(await fs.access(browserLogPath).then(() => true).catch(() => false), true);
      assert.strictEqual(await fs.access(serverLogPath).then(() => true).catch(() => false), true);
      
      // Verify session metadata
      const metadataPath = path.join(sessionDir, 'session-info.json');
      assert.strictEqual(await fs.access(metadataPath).then(() => true).catch(() => false), true);
      
      const metadata = JSON.parse(await fs.readFile(metadataPath, 'utf-8'));
      assert.strictEqual(metadata.sessionId, sessionId);
      assert.strictEqual(metadata.type, 'development');
      assert.strictEqual(metadata.owner, 'shared');
    });

    it('should return consistent log paths across multiple connect calls', async () => {
      const firstResult = await ConnectCommand.executeOperation({}, mockContext);
      const secondResult = await ConnectCommand.executeOperation({}, mockContext);
      const thirdResult = await ConnectCommand.executeOperation({}, mockContext);
      
      // All should return same log paths
      assert.strictEqual(firstResult.data.session.logPaths.browser, secondResult.data.session.logPaths.browser);
      assert.strictEqual(secondResult.data.session.logPaths.browser, thirdResult.data.session.logPaths.browser);
      
      assert.strictEqual(firstResult.data.session.logPaths.server, secondResult.data.session.logPaths.server);
      assert.strictEqual(secondResult.data.session.logPaths.server, thirdResult.data.session.logPaths.server);
      
      // Log files should actually exist
      const browserLogPath = firstResult.data.session.logPaths.browser;
      const serverLogPath = firstResult.data.session.logPaths.server;
      
      assert.strictEqual(await fs.access(browserLogPath).then(() => true).catch(() => false), true);
      assert.strictEqual(await fs.access(serverLogPath).then(() => true).catch(() => false), true);
    });
  });

  describe('Force New Session Override', () => {
    it('should create new session when forceNew=true despite existing shared session', async () => {
      // Create shared session first
      const sharedResult = await ConnectCommand.executeOperation({}, mockContext);
      assert.strictEqual(sharedResult.success, true);
      assert.strictEqual(sharedResult.data.session.action, 'created_new');
      const sharedSessionId = sharedResult.data.session.sessionId;
      
      // Force create new session
      const forceNewResult = await ConnectCommand.executeOperation({
        forceNew: true
      }, mockContext);
      assert.strictEqual(forceNewResult.success, true);
      assert.strictEqual(forceNewResult.data.session.action, 'created_new');
      const newSessionId = forceNewResult.data.session.sessionId;
      
      // Should be different sessions
      assert.strictEqual(newSessionId).not.toBe(sharedSessionId);
      
      // Next default connect should create a new shared session (since forceNew creates isolated sessions)
      const defaultResult = await ConnectCommand.executeOperation({}, mockContext);
      // Either reuse shared or create new shared session (both are valid)
      assert.strictEqual(defaultResult.success, true);
      assert.strictEqual(['joined_existing', 'created_new']).toContain(defaultResult.data.session.action);
    });
  });

  describe('Project Directory Context', () => {
    it('should include current working directory in all session responses', async () => {
      const results = await Promise.all([
        ConnectCommand.executeOperation({}, mockContext),
        ConnectCommand.executeOperation({}, mockContext), 
        ConnectCommand.executeOperation({}, mockContext)
      ]);
      
      results.forEach(result => {
        assert.strictEqual(result.success, true);
        assert.strictEqual(result.data.session.projectDirectory).toBeDefined();
        assert.strictEqual(result.data.session.projectDirectory, process.cwd());
      });
    });
  });

  describe('Session Cleanup and Lifecycle', () => {
    it('should clean up session when explicitly stopped', async () => {
      // Create session
      const result = await ConnectCommand.executeOperation({}, mockContext);
      const sessionId = result.data.session.sessionId;
      
      // Verify session exists
      const sessionDir = path.join(tempDir, 'user', 'shared', sessionId);
      assert.strictEqual(await fs.access(sessionDir).then(() => true).catch(() => false), true);
      
      // Stop session
      const stopResult = await sessionManager.stopSession(sessionId, { 
        force: true, 
        saveArtifacts: false 
      });
      assert.strictEqual(stopResult.success, true);
      
      // Session should be marked inactive
      const session = sessionManager.getSession(sessionId);
      assert.strictEqual(session?.isActive, false);
    });
  });
});