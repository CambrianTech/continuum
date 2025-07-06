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

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
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
      
      expect(firstResult.success).toBe(true);
      expect(firstResult.data.session.action).toBe('created_new');
      const firstSessionId = firstResult.data.session.sessionId;
      expect(firstSessionId).toMatch(/development-shared-/);
      
      // Test 2: Second connect call - should reuse same session
      const secondResult = await ConnectCommand.executeOperation({}, mockContext);
      
      expect(secondResult.success).toBe(true);
      expect(secondResult.data.session.action).toBe('joined_existing');
      expect(secondResult.data.session.sessionId).toBe(firstSessionId);
      
      // Test 3: Third connect call - should still reuse same session
      const thirdResult = await ConnectCommand.executeOperation({}, mockContext);
      
      expect(thirdResult.success).toBe(true);
      expect(thirdResult.data.session.action).toBe('joined_existing');
      expect(thirdResult.data.session.sessionId).toBe(firstSessionId);
      
      // Verify only ONE session directory exists
      const sessionDirs = await fs.readdir(path.join(tempDir, 'user', 'shared'));
      expect(sessionDirs).toHaveLength(1);
      expect(sessionDirs[0]).toBe(firstSessionId);
    });

    it('should handle sequential connect calls with proper session reuse', async () => {
      // Sequential connects (not simultaneous) should demonstrate proper reuse
      const firstResult = await ConnectCommand.executeOperation({}, mockContext);
      expect(firstResult.success).toBe(true);
      expect(firstResult.data.session.action).toBe('created_new');
      const sessionId = firstResult.data.session.sessionId;
      
      // Subsequent sequential calls should reuse the session
      for (let i = 0; i < 3; i++) {
        const result = await ConnectCommand.executeOperation({}, mockContext);
        expect(result.success).toBe(true);
        expect(result.data.session.sessionId).toBe(sessionId);
        expect(result.data.session.action).toBe('joined_existing');
      }
      
      // Verify only ONE session directory exists
      const sessionDirs = await fs.readdir(path.join(tempDir, 'user', 'shared'));
      expect(sessionDirs).toHaveLength(1);
      expect(sessionDirs[0]).toBe(sessionId);
    });

    it('should create different sessions for different session types', async () => {
      // Default connect (development session)
      const devResult = await ConnectCommand.executeOperation({}, mockContext);
      expect(devResult.success).toBe(true);
      const devSessionId = devResult.data.session.sessionId;
      
      // Connect with different session type
      const testResult = await ConnectCommand.executeOperation({
        sessionType: 'test'
      }, mockContext);
      expect(testResult.success).toBe(true);
      const testSessionId = testResult.data.session.sessionId;
      
      // Should be different sessions due to different types
      expect(devSessionId).not.toBe(testSessionId);
      expect(devSessionId).toMatch(/development-/);
      expect(testSessionId).toMatch(/test-/);
      
      // But reusing development should still return same dev session
      const devAgainResult = await ConnectCommand.executeOperation({
        sessionType: 'development'
      }, mockContext);
      expect(devAgainResult.data.session.sessionId).toBe(devSessionId);
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
      expect(firstResult.success).toBe(true);
      expect(firstResult.data.session.action).toBe('created_new');
      
      // Wait for async browser management
      await new Promise(resolve => setTimeout(resolve, 100));
      expect(browserLaunchCount).toBe(1); // Called once for new session
      
      // Second connect - should trigger ensure for EXISTING session
      const secondResult = await ConnectCommand.executeOperation({}, mockContext);
      expect(secondResult.success).toBe(true);
      expect(secondResult.data.session.action).toBe('joined_existing');
      
      await new Promise(resolve => setTimeout(resolve, 100));
      expect(browserLaunchCount).toBe(1); // Still only called once
      expect(browserEnsureCount).toBe(1); // Called once for existing session
      
      // Third connect - should trigger ensure again
      const thirdResult = await ConnectCommand.executeOperation({}, mockContext);
      expect(thirdResult.success).toBe(true);
      expect(thirdResult.data.session.action).toBe('joined_existing');
      
      await new Promise(resolve => setTimeout(resolve, 100));
      expect(browserLaunchCount).toBe(1); // Still only called once
      expect(browserEnsureCount).toBe(2); // Called twice for existing sessions
      
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
      expect(actualLaunchAttempts).toBe(0);
      
      // Restore original methods
      if (browserManager['tabManager'] && originalCheckTabs) {
        browserManager['tabManager'].checkTabs = originalCheckTabs;
      }
    });
  });

  describe('Session Artifact Management', () => {
    it('should create proper session directory structure for shared session', async () => {
      const result = await ConnectCommand.executeOperation({}, mockContext);
      expect(result.success).toBe(true);
      
      const sessionId = result.data.session.sessionId;
      const sessionDir = path.join(tempDir, 'user', 'shared', sessionId);
      
      // Verify session directory exists
      expect(await fs.access(sessionDir).then(() => true).catch(() => false)).toBe(true);
      
      // Verify subdirectories
      const subdirs = ['logs', 'screenshots', 'files', 'recordings', 'devtools'];
      for (const subdir of subdirs) {
        const subdirPath = path.join(sessionDir, subdir);
        expect(await fs.access(subdirPath).then(() => true).catch(() => false)).toBe(true);
      }
      
      // Verify log files exist
      const browserLogPath = path.join(sessionDir, 'logs', 'browser.log');
      const serverLogPath = path.join(sessionDir, 'logs', 'server.log');
      
      expect(await fs.access(browserLogPath).then(() => true).catch(() => false)).toBe(true);
      expect(await fs.access(serverLogPath).then(() => true).catch(() => false)).toBe(true);
      
      // Verify session metadata
      const metadataPath = path.join(sessionDir, 'session-info.json');
      expect(await fs.access(metadataPath).then(() => true).catch(() => false)).toBe(true);
      
      const metadata = JSON.parse(await fs.readFile(metadataPath, 'utf-8'));
      expect(metadata.sessionId).toBe(sessionId);
      expect(metadata.type).toBe('development');
      expect(metadata.owner).toBe('shared');
    });

    it('should return consistent log paths across multiple connect calls', async () => {
      const firstResult = await ConnectCommand.executeOperation({}, mockContext);
      const secondResult = await ConnectCommand.executeOperation({}, mockContext);
      const thirdResult = await ConnectCommand.executeOperation({}, mockContext);
      
      // All should return same log paths
      expect(firstResult.data.session.logPaths.browser).toBe(secondResult.data.session.logPaths.browser);
      expect(secondResult.data.session.logPaths.browser).toBe(thirdResult.data.session.logPaths.browser);
      
      expect(firstResult.data.session.logPaths.server).toBe(secondResult.data.session.logPaths.server);
      expect(secondResult.data.session.logPaths.server).toBe(thirdResult.data.session.logPaths.server);
      
      // Log files should actually exist
      const browserLogPath = firstResult.data.session.logPaths.browser;
      const serverLogPath = firstResult.data.session.logPaths.server;
      
      expect(await fs.access(browserLogPath).then(() => true).catch(() => false)).toBe(true);
      expect(await fs.access(serverLogPath).then(() => true).catch(() => false)).toBe(true);
    });
  });

  describe('Force New Session Override', () => {
    it('should create new session when forceNew=true despite existing shared session', async () => {
      // Create shared session first
      const sharedResult = await ConnectCommand.executeOperation({}, mockContext);
      expect(sharedResult.success).toBe(true);
      expect(sharedResult.data.session.action).toBe('created_new');
      const sharedSessionId = sharedResult.data.session.sessionId;
      
      // Force create new session
      const forceNewResult = await ConnectCommand.executeOperation({
        forceNew: true
      }, mockContext);
      expect(forceNewResult.success).toBe(true);
      expect(forceNewResult.data.session.action).toBe('created_new');
      const newSessionId = forceNewResult.data.session.sessionId;
      
      // Should be different sessions
      expect(newSessionId).not.toBe(sharedSessionId);
      
      // Next default connect should create a new shared session (since forceNew creates isolated sessions)
      const defaultResult = await ConnectCommand.executeOperation({}, mockContext);
      // Either reuse shared or create new shared session (both are valid)
      expect(defaultResult.success).toBe(true);
      expect(['joined_existing', 'created_new']).toContain(defaultResult.data.session.action);
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
        expect(result.success).toBe(true);
        expect(result.data.session.projectDirectory).toBeDefined();
        expect(result.data.session.projectDirectory).toBe(process.cwd());
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
      expect(await fs.access(sessionDir).then(() => true).catch(() => false)).toBe(true);
      
      // Stop session
      const stopResult = await sessionManager.stopSession(sessionId, { 
        force: true, 
        saveArtifacts: false 
      });
      expect(stopResult.success).toBe(true);
      
      // Session should be marked inactive
      const session = sessionManager.getSession(sessionId);
      expect(session?.isActive).toBe(false);
    });
  });
});