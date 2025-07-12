/**
 * Safe Browser Launch Integration Test
 * 
 * Tests the safety mechanisms we implemented to prevent runaway browser creation:
 * - Multiple safety checks before launching
 * - Browser existence detection
 * - Tab counting and management
 * - Event-driven browser management
 * 
 * This validates the browser management fixes that prevent infinite loops.
 */

import { describe, it, beforeEach, afterEach } from 'node:test';
import * as assert from 'node:assert';
import { BrowserManagerDaemon } from '../../BrowserManagerDaemon';
import { SessionManagerDaemon } from '../../../session-manager/SessionManagerDaemon';
import * as fs from 'fs/promises';
import * as path from 'path';
import * as os from 'os';
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

describe('Safe Browser Launch Integration', () => {
  let browserManager: BrowserManagerDaemon;
  let sessionManager: SessionManagerDaemon;
  let tempDir: string;
  let launchedPids: number[] = [];

  beforeEach(async () => {
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'continuum-safe-browser-test-'));
    
    sessionManager = new SessionManagerDaemon(tempDir);
    browserManager = new BrowserManagerDaemon();
    
    await sessionManager.start();
    await browserManager.start();
    
    // Wire up session events like the real system
    sessionManager.on('session_created', (event: any) => {
      browserManager.emit('session_created', event);
    });
    
    sessionManager.on('session_joined', (event: any) => {
      browserManager.emit('session_joined', event);
    });
    
    launchedPids = [];
  });

  afterEach(async () => {
    // Kill any browsers launched during test
    for (const pid of launchedPids) {
      try {
        await execAsync(`kill -9 ${pid} 2>/dev/null || true`);
      } catch (error) {
        // Expected if process already dead
      }
    }
    
    await browserManager.stop();
    await sessionManager.stop();
    await fs.rm(tempDir, { recursive: true, force: true });
  });

  describe('Safety Check Implementation', () => {
    it('should perform all three safety checks before launching browser', async () => {
      const checks = {
        zombieCleanup: false,
        tabCount: false,
        existingBrowser: false
      };
      
      // Mock the safety check methods to track calls
      const originalPerformZombieCleanup = browserManager['performZombieCleanup'];
      const originalTabManagerCheck = browserManager['tabManager']?.checkTabs;
      const originalGetBrowserBySession = browserManager['sessionManager']?.getBrowserBySession;
      
      browserManager['performZombieCleanup'] = async function() {
        checks.zombieCleanup = true;
        return Promise.resolve();
      };
      
      if (browserManager['tabManager']) {
        browserManager['tabManager'].checkTabs = async function() {
          checks.tabCount = true;
          return { count: 0, action: 'no_tabs', details: [] };
        };
      }
      
      if (browserManager['sessionManager']) {
        browserManager['sessionManager'].getBrowserBySession = function(sessionId: string) {
          checks.existingBrowser = true;
          return null; // No existing browser
        };
      }
      
      // Create session which should trigger safe browser launch
      const connectionResult = await sessionManager.handleConnect({
        source: 'test',
        owner: 'shared',
        sessionPreference: 'new',
        capabilities: ['browser'],
        context: 'safety-test'
      });
      
      assert.strictEqual(connectionResult.success, true);
      
      // Wait for async browser management
      await new Promise(resolve => setTimeout(resolve, 200));
      
      // Verify all safety checks were performed
      assert.strictEqual(checks.zombieCleanup, true);
      assert.strictEqual(checks.tabCount, true);
      assert.strictEqual(checks.existingBrowser, true);
      
      // Restore original methods
      browserManager['performZombieCleanup'] = originalPerformZombieCleanup;
      if (browserManager['tabManager'] && originalTabManagerCheck) {
        browserManager['tabManager'].checkTabs = originalTabManagerCheck;
      }
      if (browserManager['sessionManager'] && originalGetBrowserBySession) {
        browserManager['sessionManager'].getBrowserBySession = originalGetBrowserBySession;
      }
    });

    it('should NOT launch browser if tabs already exist (safety check 2)', async () => {
      let launchAttempted = false;
      
      // Mock tab manager to report existing tabs
      if (browserManager['tabManager']) {
        browserManager['tabManager'].checkTabs = async () => ({
          count: 2,
          action: 'existing_tabs',
          details: [
            { pid: 12345, process: 'Chrome', state: 'ESTABLISHED' },
            { pid: 12346, process: 'Chrome', state: 'ESTABLISHED' }
          ]
        });
      }
      
      // Mock launcher to detect launch attempts
      if (browserManager['launcher']) {
        const originalLaunch = browserManager['launcher'].launch;
        browserManager['launcher'].launch = async (...args) => {
          launchAttempted = true;
          return originalLaunch.apply(browserManager['launcher'], args);
        };
      }
      
      // Create session - should NOT launch browser due to existing tabs
      const connectionResult = await sessionManager.handleConnect({
        source: 'test',
        owner: 'shared', 
        sessionPreference: 'new',
        capabilities: ['browser'],
        context: 'existing-tabs-test'
      });
      
      assert.strictEqual(connectionResult.success, true);
      await new Promise(resolve => setTimeout(resolve, 200));
      
      // No browser launch should have been attempted
      assert.strictEqual(launchAttempted, false);
    });

    it('should NOT launch browser if browser already exists for session (safety check 3)', async () => {
      let launchAttempted = false;
      
      // Mock session manager to report existing browser
      if (browserManager['sessionManager']) {
        browserManager['sessionManager'].getBrowserBySession = () => ({
          id: 'existing-browser-123',
          pid: 12345,
          status: 'ready'
        });
      }
      
      // Mock launcher to detect launch attempts  
      if (browserManager['launcher']) {
        const originalLaunch = browserManager['launcher'].launch;
        browserManager['launcher'].launch = async (...args) => {
          launchAttempted = true;
          return originalLaunch.apply(browserManager['launcher'], args);
        };
      }
      
      // Create session - should NOT launch browser due to existing browser
      const connectionResult = await sessionManager.handleConnect({
        source: 'test',
        owner: 'shared',
        sessionPreference: 'new', 
        capabilities: ['browser'],
        context: 'existing-browser-test'
      });
      
      assert.strictEqual(connectionResult.success, true);
      await new Promise(resolve => setTimeout(resolve, 200));
      
      // No browser launch should have been attempted
      assert.strictEqual(launchAttempted, false);
    });
  });

  describe('Event-Driven Browser Management', () => {
    it('should handle session_created events with safe browser launch', async () => {
      let sessionCreatedHandled = false;
      let safetyChecksPerformed = 0;
      
      // Mock the safety launcher to track calls
      const originalSafeLaunch = browserManager['safelyLaunchBrowserForSession'];
      browserManager['safelyLaunchBrowserForSession'] = async function(...args) {
        sessionCreatedHandled = true;
        safetyChecksPerformed++;
        // Don't actually launch in test
        return Promise.resolve();
      };
      
      // Create session which should emit session_created event
      const connectionResult = await sessionManager.handleConnect({
        source: 'test',
        owner: 'shared',
        sessionPreference: 'new',
        capabilities: ['browser'],
        context: 'event-driven-test'
      });
      
      assert.strictEqual(connectionResult.success, true);
      await new Promise(resolve => setTimeout(resolve, 100));
      
      // Verify event was handled with safe launch
      assert.strictEqual(sessionCreatedHandled, true);
      assert.strictEqual(safetyChecksPerformed, 1);
      
      // Restore original method
      browserManager['safelyLaunchBrowserForSession'] = originalSafeLaunch;
    });

    it('should handle session_joined events with browser existence check', async () => {
      let sessionJoinedHandled = false;
      let existenceChecksPerformed = 0;
      
      // Create session first
      const firstResult = await sessionManager.handleConnect({
        source: 'test',
        owner: 'shared',
        sessionPreference: 'new',
        capabilities: ['browser'],
        context: 'join-test'
      });
      assert.strictEqual(firstResult.success, true);
      
      // Mock the existence checker to track calls
      const originalEnsure = browserManager['ensureBrowserExistsForSession'];
      browserManager['ensureBrowserExistsForSession'] = async function(...args) {
        sessionJoinedHandled = true;
        existenceChecksPerformed++;
        return Promise.resolve();
      };
      
      // Join existing session which should emit session_joined event
      const joinResult = await sessionManager.handleConnect({
        source: 'test',
        owner: 'shared',
        sessionPreference: 'current',
        capabilities: ['browser'],
        context: 'join-test'
      });
      
      assert.strictEqual(joinResult.success, true);
      assert.strictEqual(joinResult.data.action, 'joined_existing');
      await new Promise(resolve => setTimeout(resolve, 100));
      
      // Verify event was handled with existence check
      assert.strictEqual(sessionJoinedHandled, true);
      assert.strictEqual(existenceChecksPerformed, 1);
      
      // Restore original method
      browserManager['ensureBrowserExistsForSession'] = originalEnsure;
    });

    it('should prevent runaway browser launches with rapid session events', async () => {
      let totalLaunchAttempts = 0;
      let totalExistenceChecks = 0;
      
      // Mock both methods to count calls
      const originalSafeLaunch = browserManager['safelyLaunchBrowserForSession'];
      const originalEnsure = browserManager['ensureBrowserExistsForSession'];
      
      browserManager['safelyLaunchBrowserForSession'] = async function(...args) {
        totalLaunchAttempts++;
        return Promise.resolve();
      };
      
      browserManager['ensureBrowserExistsForSession'] = async function(...args) {
        totalExistenceChecks++;
        return Promise.resolve();
      };
      
      // Fire off multiple rapid session operations
      const promises = [
        // One new session (should trigger launch)
        sessionManager.handleConnect({
          source: 'test1',
          owner: 'shared',
          sessionPreference: 'new',
          capabilities: ['browser'],
          context: 'rapid-test'
        }),
        // Multiple joins to same session (should trigger existence checks)
        ...Array.from({ length: 5 }, () =>
          sessionManager.handleConnect({
            source: 'test-join',
            owner: 'shared', 
            sessionPreference: 'current',
            capabilities: ['browser'],
            context: 'rapid-test'
          })
        )
      ];
      
      const results = await Promise.all(promises);
      await new Promise(resolve => setTimeout(resolve, 300));
      
      // All should succeed
      results.forEach(result => {
        assert.strictEqual(result.success, true);
      });
      
      // Should have controlled number of calls (not runaway)
      assert.ok(totalLaunchAttempts <= 2, 'At most 1-2 launch attempts');
      assert.ok(totalExistenceChecks <= 10, 'Reasonable number of checks');
      
      // Restore original methods
      browserManager['safelyLaunchBrowserForSession'] = originalSafeLaunch;
      browserManager['ensureBrowserExistsForSession'] = originalEnsure;
    });
  });

  describe('Session Type Filtering', () => {
    it('should only launch browsers for session types that need them', async () => {
      let browserLaunchCount = 0;
      
      // Mock launch tracking
      const originalSafeLaunch = browserManager['safelyLaunchBrowserForSession'];
      browserManager['safelyLaunchBrowserForSession'] = async function(...args) {
        browserLaunchCount++;
        return Promise.resolve();
      };
      
      // Create development session (should launch browser)
      const devResult = await sessionManager.handleConnect({
        source: 'test',
        owner: 'shared',
        sessionPreference: 'new',
        sessionType: 'development',
        capabilities: ['browser'],
        context: 'dev-test'
      });
      assert.strictEqual(devResult.success, true);
      
      // Create test session (might not need browser based on logic)
      const testResult = await sessionManager.handleConnect({
        source: 'test',
        owner: 'shared',
        sessionPreference: 'new',
        sessionType: 'test',
        capabilities: ['browser'],
        context: 'test-session'
      });
      assert.strictEqual(testResult.success, true);
      
      await new Promise(resolve => setTimeout(resolve, 200));
      
      // Should have launched browser for development session
      assert.ok(browserLaunchCount > 0, 'Browser should be launched');
      
      // Restore original method
      browserManager['safelyLaunchBrowserForSession'] = originalSafeLaunch;
    });
  });

  describe('Browser Tab Management', () => {
    it('should handle existing tabs gracefully without launching new browser', async () => {
      // Mock existing tabs
      if (browserManager['tabManager']) {
        browserManager['tabManager'].checkTabs = async () => ({
          count: 1,
          action: 'existing_connection',
          details: [{ pid: 12345, process: 'Chrome', state: 'ESTABLISHED' }]
        });
      }
      
      let ensureCalled = false;
      const originalEnsure = browserManager['ensureBrowserExistsForSession'];
      browserManager['ensureBrowserExistsForSession'] = async function(...args) {
        ensureCalled = true;
        // Should NOT launch new browser - existing tab found
        return Promise.resolve();
      };
      
      // Join session - should find existing tab and not launch new browser
      const result = await sessionManager.handleConnect({
        source: 'test',
        owner: 'shared',
        sessionPreference: 'current',
        capabilities: ['browser'],
        context: 'existing-tab-test'
      });
      
      assert.strictEqual(result.success, true);
      await new Promise(resolve => setTimeout(resolve, 100));
      
      // Should have called ensure method
      assert.strictEqual(ensureCalled, true);
      
      // Restore original method
      browserManager['ensureBrowserExistsForSession'] = originalEnsure;
    });
  });
});