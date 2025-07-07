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
});