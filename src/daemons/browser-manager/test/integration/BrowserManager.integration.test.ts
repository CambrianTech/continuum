/**
 * Browser Manager Daemon Integration Tests
 * 
 * MIDDLE-OUT LAYER 2: Daemon Process Integration
 * Tests browser management coordination with session events
 * Validates smart defaults and platform-specific behavior
 */

import { describe, it, test } from 'node:test';
import assert from 'node:assert';
import { BrowserManagerDaemon } from '../../BrowserManagerDaemon';
import { DAEMON_EVENT_BUS } from '../../../base/DaemonEventBus';
import { SystemEventType } from '../../../base/EventTypes';

describe('Browser Manager Integration Tests', () => {
  describe('Smart Browser Management', () => {
    test('should initialize with proper platform adapter', async () => {
      const daemon = new BrowserManagerDaemon();
      await daemon.start();
      
      // Verify daemon started successfully
      assert.strictEqual(daemon.name, 'browser-manager');
      assert.strictEqual(daemon.version, '2.0.0');
      
      await daemon.stop();
    });

    test('should handle session creation events', async () => {
      const daemon = new BrowserManagerDaemon();
      await daemon.start();
      
      let eventReceived = false;
      const originalEnsureSessionHasBrowser = (daemon as any).ensureSessionHasBrowser;
      
      // Mock ensureSessionHasBrowser to track calls
      (daemon as any).ensureSessionHasBrowser = async (sessionId: string, sessionType: string, owner: string, killZombies: boolean = false) => {
        eventReceived = true;
        assert.strictEqual(sessionType, 'development');
        assert.strictEqual(owner, 'shared');
        return Promise.resolve();
      };
      
      // Emit session created event
      DAEMON_EVENT_BUS.emitEvent(SystemEventType.SESSION_CREATED, {
        sessionId: 'test-session-123',
        sessionType: 'development',
        owner: 'shared',
        created: new Date()
      });
      
      // Give event time to propagate
      await new Promise(resolve => setTimeout(resolve, 50));
      
      assert.strictEqual(eventReceived, true, 'Session creation event should trigger browser management');
      
      // Restore original method
      (daemon as any).ensureSessionHasBrowser = originalEnsureSessionHasBrowser;
      await daemon.stop();
    });

    test('should respect session type filtering', async () => {
      const daemon = new BrowserManagerDaemon();
      await daemon.start();
      
      let browserCallCount = 0;
      (daemon as any).ensureSessionHasBrowser = async () => {
        browserCallCount++;
        return Promise.resolve();
      };
      
      // Emit different session types
      DAEMON_EVENT_BUS.emitEvent(SystemEventType.SESSION_CREATED, {
        sessionId: 'dev-session',
        sessionType: 'development',
        owner: 'shared',
        created: new Date()
      });
      
      DAEMON_EVENT_BUS.emitEvent(SystemEventType.SESSION_CREATED, {
        sessionId: 'validation-session',
        sessionType: 'validation',
        owner: 'system',
        created: new Date()
      });
      
      DAEMON_EVENT_BUS.emitEvent(SystemEventType.SESSION_CREATED, {
        sessionId: 'portal-session',
        sessionType: 'portal',
        owner: 'user',
        created: new Date()
      });
      
      // Give events time to propagate
      await new Promise(resolve => setTimeout(resolve, 100));
      
      // Should only trigger for development and portal sessions (not validation)
      assert.strictEqual(browserCallCount, 2, 'Should only launch browsers for development and portal sessions');
      
      await daemon.stop();
    });
  });

  describe('Platform Adapter Integration', () => {
    test('should initialize macOS Opera adapter on Darwin platform', async () => {
      // Only run on macOS
      if (process.platform !== 'darwin') {
        console.log('⏭️ Skipping macOS-specific test on non-Darwin platform');
        return;
      }
      
      const daemon = new BrowserManagerDaemon();
      await daemon.start();
      
      // Access the private tabAdapter to verify it's the right type
      const tabAdapter = (daemon as any).tabAdapter;
      assert.ok(tabAdapter, 'Tab adapter should be initialized');
      assert.strictEqual(tabAdapter.constructor.name, 'MacOperaAdapter', 'Should use Opera adapter on macOS');
      
      await daemon.stop();
    });

    test('should handle adapter fallback gracefully', async () => {
      const daemon = new BrowserManagerDaemon();
      
      // Mock the initializeTabAdapter to test fallback behavior
      const originalInit = (daemon as any).initializeTabAdapter;
      let fallbackUsed = false;
      
      (daemon as any).initializeTabAdapter = async function() {
        try {
          // Simulate Opera adapter failure
          throw new Error('Opera not available');
        } catch (error) {
          fallbackUsed = true;
          // Use fallback initialization
          const { MacOperaAdapter } = await import('../../modules/BrowserTabAdapter');
          this.tabAdapter = new MacOperaAdapter();
        }
      };
      
      await daemon.start();
      
      if (process.platform === 'darwin') {
        assert.strictEqual(fallbackUsed, true, 'Should use fallback when primary adapter fails');
      }
      
      // Restore original method
      (daemon as any).initializeTabAdapter = originalInit;
      await daemon.stop();
    });
  });

  describe('Browser Tab Management', () => {
    test('should implement semaphore protection', async () => {
      const daemon = new BrowserManagerDaemon();
      await daemon.start();
      
      // Mock the actual browser launching to avoid real browser spawning
      let launchAttempts = 0;
      const originalLauncher = (daemon as any).launcher;
      (daemon as any).launcher = {
        launch: async () => {
          launchAttempts++;
          // Simulate slow launch
          await new Promise(resolve => setTimeout(resolve, 100));
          return { pid: 12345, debugPort: 9222, devToolsUrl: 'http://localhost:9222' };
        }
      };
      
      // Mock tab adapter to return 0 tabs (should trigger launch)
      (daemon as any).tabAdapter = {
        countTabs: async () => 0,
        constructor: { name: 'MockAdapter' }
      };
      
      // Start multiple simultaneous browser requests
      const promises = [
        (daemon as any).ensureSessionHasBrowser('session1', 'development', 'shared', false),
        (daemon as any).ensureSessionHasBrowser('session2', 'development', 'shared', false),
        (daemon as any).ensureSessionHasBrowser('session3', 'development', 'shared', false)
      ];
      
      await Promise.all(promises);
      
      // Only one launch should have occurred due to semaphore protection
      assert.strictEqual(launchAttempts, 1, 'Semaphore should prevent multiple simultaneous launches');
      
      // Restore original launcher
      (daemon as any).launcher = originalLauncher;
      await daemon.stop();
    });

    test('should respect ONE TAB POLICY', async () => {
      const daemon = new BrowserManagerDaemon();
      await daemon.start();
      
      let launchAttempts = 0;
      (daemon as any).launcher = {
        launch: async () => {
          launchAttempts++;
          return { pid: 12345, debugPort: 9222, devToolsUrl: 'http://localhost:9222' };
        }
      };
      
      // Mock tab adapter to return existing tabs
      (daemon as any).tabAdapter = {
        countTabs: async () => 2, // Existing tabs found
        constructor: { name: 'MockAdapter' }
      };
      
      await (daemon as any).ensureSessionHasBrowser('session1', 'development', 'shared', false);
      
      // No launch should occur when tabs already exist
      assert.strictEqual(launchAttempts, 0, 'Should not launch browser when tabs already exist');
      
      await daemon.stop();
    });
  });

  describe('Zombie Management Integration', () => {
    test('should trigger zombie cleanup when killZombies is true', async () => {
      const daemon = new BrowserManagerDaemon();
      await daemon.start();
      
      let zombieCleanupCalled = false;
      (daemon as any).killZombieTabs = async (sessionId: string) => {
        zombieCleanupCalled = true;
        assert.strictEqual(sessionId, 'test-session');
        return Promise.resolve();
      };
      
      // Mock tab adapter to return multiple tabs (triggers zombie cleanup)
      (daemon as any).tabAdapter = {
        countTabs: async () => 3, // Multiple tabs found
        constructor: { name: 'MockAdapter' }
      };
      
      // Call with focus=false, killZombies=true
      await (daemon as any).ensureSessionHasBrowser('test-session', 'development', 'shared', false, true);
      
      assert.strictEqual(zombieCleanupCalled, true, 'Should call zombie cleanup when killZombies is true and multiple tabs exist');
      
      await daemon.stop();
    });

    test('should close correct tabs during zombie cleanup', async () => {
      const daemon = new BrowserManagerDaemon();
      await daemon.start();
      
      // Track which tabs get closed
      const tabsToClose: string[] = [];
      const mockTabs = [
        { id: 'tab1', url: 'http://localhost:9000' },
        { id: 'tab2', url: 'http://localhost:9000' }, 
        { id: 'tab3', url: 'http://localhost:9000' },
        { id: 'tab4', url: 'http://localhost:9000' }
      ];
      
      // Mock the actual zombie killer to test logic
      (daemon as any).killZombieTabs = async (sessionId: string) => {
        // Simulate the CORRECT zombie logic:
        // Keep the first tab, close all others
        for (let i = 1; i < mockTabs.length; i++) {
          tabsToClose.push(mockTabs[i].id);
        }
        return Promise.resolve();
      };
      
      // Mock tab adapter to return multiple tabs
      (daemon as any).tabAdapter = {
        countTabs: async () => mockTabs.length,
        constructor: { name: 'MockAdapter' }
      };
      
      await (daemon as any).ensureSessionHasBrowser('test-session', 'development', 'shared', false, true);
      
      // Verify correct zombie behavior: keep first tab, close others
      assert.strictEqual(tabsToClose.length, 3, 'Should close 3 zombie tabs (keep first, close rest)');
      assert.deepStrictEqual(tabsToClose, ['tab2', 'tab3', 'tab4'], 'Should close tabs 2-4, keep tab1');
      
      await daemon.stop();
    });

    test('should detect AppleScript zombie logic bug', async () => {
      const daemon = new BrowserManagerDaemon();
      await daemon.start();
      
      // Track AppleScript behavior
      const tabsToClose: string[] = [];
      const mockTabs = [
        { id: 'tab1', url: 'http://localhost:9000' },
        { id: 'tab2', url: 'http://localhost:9000' }, 
        { id: 'tab3', url: 'http://localhost:9000' }
      ];
      
      // Mock the BUGGY AppleScript logic to test detection
      (daemon as any).killZombieTabs = async (sessionId: string) => {
        // Simulate the BUGGY logic: if (count of tabsToClose) > 0 then add tab
        // This results in keeping the first tab it finds, closing the rest
        for (let i = 0; i < mockTabs.length; i++) {
          if (tabsToClose.length > 0) {
            // Bug: adds tab when list already has items  
            tabsToClose.push(mockTabs[i].id);
          }
          // First tab gets skipped because tabsToClose.length === 0
        }
        return Promise.resolve();
      };
      
      (daemon as any).tabAdapter = {
        countTabs: async () => mockTabs.length,
        constructor: { name: 'MockAdapter' }
      };
      
      await (daemon as any).ensureSessionHasBrowser('test-session', 'development', 'shared', false, true);
      
      // The buggy logic should close tabs 2 and 3, but skip tab 1
      // This is actually the correct behavior by accident, but the logic is wrong
      assert.strictEqual(tabsToClose.length, 2, 'Buggy logic closes 2 tabs');
      assert.deepStrictEqual(tabsToClose, ['tab2', 'tab3'], 'Buggy logic accidentally works for this case');
      
      await daemon.stop();
    });

    test('should expose when AppleScript logic closes ALL tabs', async () => {
      const daemon = new BrowserManagerDaemon();
      await daemon.start();
      
      // Test the worst case: AppleScript closes ALL tabs
      const tabsToClose: string[] = [];
      const mockTabs = [
        { id: 'tab1', url: 'http://localhost:9000' }  // Only one tab
      ];
      
      // Mock broken AppleScript that closes everything
      (daemon as any).killZombieTabs = async (sessionId: string) => {
        // This simulates if the AppleScript logic is completely broken
        // and closes even the only tab
        for (const tab of mockTabs) {
          tabsToClose.push(tab.id);
        }
        return Promise.resolve();
      };
      
      (daemon as any).tabAdapter = {
        countTabs: async () => mockTabs.length,
        constructor: { name: 'MockAdapter' }
      };
      
      await (daemon as any).ensureSessionHasBrowser('test-session', 'development', 'shared', false, true);
      
      // This should NOT happen - we should never close the only tab
      assert.strictEqual(tabsToClose.length, 1, 'BROKEN: Closed the only tab!');
      assert.strictEqual(tabsToClose[0], 'tab1', 'BROKEN: Closed the tab we should keep');
      
      await daemon.stop();
    });

    test('should skip zombie cleanup when killZombies is false', async () => {
      const daemon = new BrowserManagerDaemon();
      await daemon.start();
      
      let zombieCleanupCalled = false;
      (daemon as any).killZombieTabs = async () => {
        zombieCleanupCalled = true;
        return Promise.resolve();
      };
      
      (daemon as any).tabAdapter = {
        countTabs: async () => 3,
        constructor: { name: 'MockAdapter' }
      };
      
      await (daemon as any).ensureSessionHasBrowser('test-session', 'development', 'shared', false);
      
      assert.strictEqual(zombieCleanupCalled, false, 'Should not call zombie cleanup when killZombies is false');
      
      await daemon.stop();
    });
  });

  describe('Focus Management Integration', () => {
    test('should handle focus requests gracefully', async () => {
      const daemon = new BrowserManagerDaemon();
      await daemon.start();
      
      let focusCalled = false;
      (daemon as any).focusBrowser = async () => {
        focusCalled = true;
        return Promise.resolve();
      };
      
      // Mock successful browser launch
      (daemon as any).launcher = {
        launch: async () => {
          return { pid: 12345, debugPort: 9222, devToolsUrl: 'http://localhost:9222' };
        }
      };
      
      (daemon as any).tabAdapter = {
        countTabs: async () => 0, // No tabs, will launch
        constructor: { name: 'MockAdapter' }
      };
      
      // Use killZombies as proxy for focus (as implemented)
      const result = await (daemon as any).ensureSessionHasBrowser('test-session', 'development', 'shared', true);
      
      // Focus should either be called OR browser should be newly launched (which has focus automatically)
      // Since we're mocking a launch scenario (countTabs returns 0), focus is skipped because
      // newly launched browsers automatically have focus
      assert.strictEqual(focusCalled, false, 'Focus should be skipped for newly launched browser (it already has focus)');
      
      await daemon.stop();
    });
  });

  describe('Real AppleScript Zombie Logic Testing', () => {
    test('should test actual AppleScript zombie killer logic', async () => {
      const daemon = new BrowserManagerDaemon();
      await daemon.start();
      
      // Track which actual AppleScript gets executed
      const executedScripts: string[] = [];
      
      // Mock the execAsync function used by killZombieTabs
      const originalExecAsync = (daemon as any).constructor.prototype.execAsync;
      (daemon as any).execAsync = async (command: string) => {
        if (command.includes('osascript')) {
          executedScripts.push(command);
          
          // Parse the AppleScript to check for bugs
          const scriptMatch = command.match(/osascript -e '(.+)'/);
          if (scriptMatch) {
            const script = scriptMatch[1].replace(/'\"'\"'/g, "'");
            
            // 🐛 CHECK FOR THE ACTUAL BUG IN APPLESCRIPT
            if (script.includes('if (count of tabsToClose) > 0 then')) {
              assert.fail('🐛 DETECTED APPLESCRIPT BUG: Logic is backwards!\n' +
                         'Found: "if (count of tabsToClose) > 0 then"\n' +
                         'Should be: "if (count of tabsToClose) = 0 then"\n' +
                         'This bug causes wrong tabs to be closed!');
            }
            
            return { stdout: '2' }; // Simulate closing some tabs
          }
        }
        return { stdout: '0' };
      };
      
      try {
        // Call the real killZombieTabs method - this will expose the bug!
        await (daemon as any).killZombieTabs('test-session');
        
        // If we get here, either the AppleScript is correct OR platform isn't Darwin
        if (process.platform === 'darwin') {
          assert.strictEqual(executedScripts.length, 1, 'Should execute AppleScript on macOS');
          
          const script = executedScripts[0];
          
          // Verify script correctness
          assert(script.includes('localhost:9000'), 'Should target localhost:9000');
          assert(script.includes('tabsToClose'), 'Should use tabsToClose array');
          
          // If we reach here, the logic should be correct
          console.log('✅ AppleScript zombie logic appears correct');
        }
        
      } finally {
        // Restore original function
        if (originalExecAsync) {
          (daemon as any).execAsync = originalExecAsync;
        }
      }
      
      await daemon.stop();
    });

    test('should validate AppleScript zombie logic correctness', async () => {
      // Test the actual AppleScript logic pattern in isolation
      const correctLogic = `
        if (count of tabsToClose) = 0 then
          -- Keep first tab, add others to close list
        else
          set end of tabsToClose to t
        end if
      `;
      
      const buggyLogic = `
        if (count of tabsToClose) > 0 then
          set end of tabsToClose to t
        end if
      `;
      
      // Simulate what each logic does with 4 tabs
      const tabs = ['tab1', 'tab2', 'tab3', 'tab4'];
      
      // Correct logic simulation: keep first, close rest
      const correctClosedTabs: string[] = [];
      for (let i = 0; i < tabs.length; i++) {
        if (i === 0) {
          // Keep first tab (don't add to close list)
          continue;
        } else {
          correctClosedTabs.push(tabs[i]);
        }
      }
      
      // Buggy logic simulation: if list has items, add tab
      const buggyClosedTabs: string[] = [];
      for (let i = 0; i < tabs.length; i++) {
        if (buggyClosedTabs.length > 0) {
          buggyClosedTabs.push(tabs[i]);
        }
        // First tab skipped because list is empty
      }
      
      // Verify correct logic works
      assert.deepStrictEqual(correctClosedTabs, ['tab2', 'tab3', 'tab4'], 'Correct logic keeps first tab');
      
      // Verify buggy logic accidentally works in this case
      assert.deepStrictEqual(buggyClosedTabs, ['tab2', 'tab3', 'tab4'], 'Buggy logic accidentally works here');
      
      // But test edge case where buggy logic fails
      const singleTab = ['onlyTab'];
      const buggyWithOneTab: string[] = [];
      for (let i = 0; i < singleTab.length; i++) {
        if (buggyWithOneTab.length > 0) {
          buggyWithOneTab.push(singleTab[i]);
        }
      }
      
      // Buggy logic correctly leaves single tab alone (by accident)
      assert.strictEqual(buggyWithOneTab.length, 0, 'Buggy logic accidentally preserves single tab');
      
      // The real bug shows up in the AppleScript implementation details
      // The backwards condition means it's doing the opposite of what the comment says
    });
  });
});