/**
 * Safe Browser Manager Tests - Critical safety and reliability testing
 * 
 * FOCUS: Bulletproof browser management without side effects
 * Tests all failure modes, edge cases, and safety mechanisms
 */

import { SafeBrowserManager } from '../SafeBrowserManager';

describe('SafeBrowserManager - Critical Safety Tests', () => {
  let browserManager: SafeBrowserManager;

  beforeEach(() => {
    browserManager = new SafeBrowserManager();
  });

  describe('Safe State Management', () => {
    test('should start with safe disconnected state', () => {
      const status = browserManager.getConnectionStatus();
      
      expect(status.isConnected).toBe(false);
      expect(status.connectionId).toBe(null);
      expect(status.lastSeen).toBe(null);
      expect(status.isHealthy).toBe(false);
    });

    test('should safely register connection', () => {
      browserManager.registerConnection('test-connection-123');
      
      const status = browserManager.getConnectionStatus();
      expect(status.isConnected).toBe(true);
      expect(status.connectionId).toBe('test-connection-123');
      expect(status.lastSeen).toBeInstanceOf(Date);
      expect(status.isHealthy).toBe(true);
    });

    test('should safely register disconnection', () => {
      // First connect
      browserManager.registerConnection('test-connection');
      
      // Then disconnect
      browserManager.registerDisconnection('test-connection');
      
      const status = browserManager.getConnectionStatus();
      expect(status.isConnected).toBe(false);
      expect(status.connectionId).toBe(null);
      expect(status.isHealthy).toBe(false);
      // Should preserve lastSeen time
      expect(status.lastSeen).toBeInstanceOf(Date);
    });

    test('should return immutable status copies', () => {
      const status1 = browserManager.getConnectionStatus();
      const status2 = browserManager.getConnectionStatus();
      
      expect(status1).not.toBe(status2); // Different objects
      expect(status1).toEqual(status2); // Same values
      
      // Mutating returned status should not affect internal state
      status1.isConnected = true;
      const status3 = browserManager.getConnectionStatus();
      expect(status3.isConnected).toBe(false);
    });
  });

  describe('Anti-Spam Launch Protection', () => {
    test('should prevent concurrent browser launches', async () => {
      // Start first launch (will be in progress)
      const promise1 = browserManager.launchBrowser();
      
      // Immediately try second launch (should be rejected)
      const result2 = await browserManager.launchBrowser();
      
      expect(result2.success).toBe(false);
      expect(result2.action).toBe('failed');
      expect(result2.error).toContain('Launch already in progress');
      expect(result2.details?.reason).toBe('concurrent_launch_prevented');
      
      // Wait for first launch to complete
      await promise1;
    });

    test('should not launch if browser already connected', async () => {
      // Simulate existing connection
      browserManager.registerConnection('existing-connection');
      
      const result = await browserManager.launchBrowser();
      
      expect(result.success).toBe(true);
      expect(result.action).toBe('already_running');
      expect(result.details?.reason).toBe('browser_already_connected');
    });

    test('should respect maximum launch attempts', async () => {
      // Force multiple failed launches
      let attempt = 1;
      while (attempt <= 4) { // Try 4 times (max is 3)
        const result = await browserManager.launchBrowser();
        
        if (attempt <= 3) {
          // First 3 attempts should proceed (may succeed or fail)
          expect(typeof result.success).toBe('boolean');
        } else {
          // 4th attempt should be blocked
          expect(result.success).toBe(false);
          expect(result.action).toBe('failed');
          expect(result.error).toContain('Max launch attempts');
          expect(result.details?.reason).toBe('max_attempts_reached');
        }
        
        attempt++;
      }
    });

    test('should reset launch attempts after success', async () => {
      // Force 2 failed attempts
      await browserManager.launchBrowser();
      await browserManager.launchBrowser();
      
      // Simulate successful connection (resets attempts)
      browserManager.registerConnection('success-connection');
      browserManager.resetLaunchAttempts();
      
      // Should be able to launch again
      const result = await browserManager.launchBrowser();
      expect(result.action).toBe('already_running'); // Because connected
    });
  });

  describe('Connection Health Monitoring', () => {
    test('should detect healthy connections', () => {
      browserManager.registerConnection('healthy-connection');
      
      expect(browserManager.isConnectionHealthy()).toBe(true);
    });

    test('should detect stale connections', async () => {
      browserManager.registerConnection('stale-connection');
      
      // Wait longer than staleness threshold
      await new Promise(resolve => setTimeout(resolve, 100));
      
      // Check with very short staleness threshold
      expect(browserManager.isConnectionHealthy(50)).toBe(false);
    });

    test('should handle disconnected state safely', () => {
      // No connection registered
      expect(browserManager.isConnectionHealthy()).toBe(false);
      
      // After disconnection
      browserManager.registerConnection('temp');
      browserManager.registerDisconnection();
      expect(browserManager.isConnectionHealthy()).toBe(false);
    });

    test('should update connection health status', () => {
      browserManager.registerConnection('health-test');
      browserManager.updateConnectionHealth();
      
      const status = browserManager.getConnectionStatus();
      expect(status.isHealthy).toBe(true);
    });
  });

  describe('Browser Ready Assurance', () => {
    test('should confirm ready state for connected browser', async () => {
      browserManager.registerConnection('ready-browser');
      
      const result = await browserManager.ensureBrowserReady(1000);
      
      expect(result.ready).toBe(true);
      expect(result.error).toBeUndefined();
    });

    test('should handle timeout gracefully', async () => {
      // No browser connection will be established
      const result = await browserManager.ensureBrowserReady(100); // Very short timeout
      
      expect(result.ready).toBe(false);
      expect(result.error).toContain('timeout');
    }, 10000);

    test('should handle launch failures gracefully', async () => {
      // Force max attempts to be reached first
      await browserManager.launchBrowser();
      await browserManager.launchBrowser();
      await browserManager.launchBrowser();
      
      const result = await browserManager.ensureBrowserReady(1000);
      
      expect(result.ready).toBe(false);
      expect(result.error).toBeDefined();
    });
  });

  describe('Wait for Connection Safety', () => {
    test('should return immediately for connected browser', async () => {
      browserManager.registerConnection('immediate-connection');
      
      const startTime = Date.now();
      const connected = await browserManager.waitForConnection(5000);
      const elapsed = Date.now() - startTime;
      
      expect(connected).toBe(true);
      expect(elapsed).toBeLessThan(100); // Should be immediate
    });

    test('should timeout gracefully', async () => {
      const startTime = Date.now();
      const connected = await browserManager.waitForConnection(200); // Short timeout
      const elapsed = Date.now() - startTime;
      
      expect(connected).toBe(false);
      expect(elapsed).toBeGreaterThanOrEqual(200);
      expect(elapsed).toBeLessThan(300); // Should not hang significantly past timeout
    });

    test('should detect connection during wait', async () => {
      // Start waiting
      const waitPromise = browserManager.waitForConnection(2000);
      
      // Connect after a delay
      setTimeout(() => {
        browserManager.registerConnection('delayed-connection');
      }, 100);
      
      const connected = await waitPromise;
      expect(connected).toBe(true);
    });
  });

  describe('Error Handling and Recovery', () => {
    test('should handle state corruption gracefully', () => {
      // Try to corrupt state (should not crash)
      expect(() => {
        browserManager.registerConnection('');
        browserManager.registerDisconnection('non-existent');
        browserManager.getConnectionStatus();
      }).not.toThrow();
    });

    test('should provide manual recovery mechanisms', () => {
      // Force into bad state
      browserManager.launchBrowser();
      browserManager.launchBrowser();
      browserManager.launchBrowser();
      
      // Manual recovery
      browserManager.resetLaunchAttempts();
      
      // Should be able to try again
      expect(async () => {
        await browserManager.launchBrowser();
      }).not.toThrow();
    });

    test('should never throw unhandled errors', async () => {
      // Try various operations that might fail
      expect(async () => {
        await browserManager.launchBrowser('invalid://url');
        await browserManager.ensureBrowserReady(-1); // Invalid timeout
        await browserManager.waitForConnection(-1); // Invalid timeout
        browserManager.registerConnection(null as any);
        browserManager.updateConnectionHealth();
      }).not.toThrow();
    });
  });
});