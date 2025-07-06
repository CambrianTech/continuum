/**
 * Browser Connection Tests - Isolated connection process testing
 * 
 * FOCUS: Just the connection flow, not full system integration
 * Tests the smart browser launching and connection awareness
 */

import { BrowserManagerDaemon } from '../BrowserManagerDaemon';

describe('Browser Connection Process', () => {
  let browserManager: BrowserManagerDaemon;

  beforeEach(async () => {
    browserManager = new BrowserManagerDaemon();
    await browserManager.start();
  });

  afterEach(async () => {
    await browserManager.stop();
  });

  describe('Connection State Management', () => {
    test('should start with disconnected state', async () => {
      const response = await browserManager.handleMessage({
        type: 'get_browser_status',
        data: {}
      });

      expect(response.success).toBe(true);
      expect(response.data.state.isConnected).toBe(false);
      expect(response.data.state.isLaunched).toBe(false);
    });

    test('should track browser connection state', async () => {
      // Simulate browser connection
      const connectResponse = await browserManager.handleMessage({
        type: 'browser_connected',
        data: { connectionId: 'test-connection-123' }
      });

      expect(connectResponse.success).toBe(true);
      expect(connectResponse.data.state.isConnected).toBe(true);
      expect(connectResponse.data.state.connectionId).toBe('test-connection-123');
    });

    test('should handle browser disconnection', async () => {
      // First connect
      await browserManager.handleMessage({
        type: 'browser_connected', 
        data: { connectionId: 'test-connection-123' }
      });

      // Then disconnect
      const disconnectResponse = await browserManager.handleMessage({
        type: 'browser_disconnected',
        data: { connectionId: 'test-connection-123' }
      });

      expect(disconnectResponse.success).toBe(true);
      expect(disconnectResponse.data.state.isConnected).toBe(false);
      expect(disconnectResponse.data.state.connectionId).toBe(null);
    });
  });

  describe('Smart Browser Launching', () => {
    test('should not launch if already connected', async () => {
      // Simulate existing connection
      await browserManager.handleMessage({
        type: 'browser_connected',
        data: { connectionId: 'existing-connection' }
      });

      // Try to launch browser
      const launchResponse = await browserManager.handleMessage({
        type: 'launch_browser',
        data: { url: 'http://localhost:9000' }
      });

      expect(launchResponse.success).toBe(true);
      expect(launchResponse.data.action).toBe('already_connected');
    });

    test('should launch browser when not connected', async () => {
      const launchResponse = await browserManager.handleMessage({
        type: 'launch_browser',
        data: { url: 'http://localhost:9000' }
      });

      expect(launchResponse.success).toBe(true);
      expect(['launched', 'waiting_for_connection']).toContain(launchResponse.data.action);
    });

    test('should wait for connection after launch', async () => {
      // Launch browser first
      await browserManager.handleMessage({
        type: 'launch_browser',
        data: { url: 'http://localhost:9000' }
      });

      // Immediate second launch should wait
      const secondLaunchResponse = await browserManager.handleMessage({
        type: 'launch_browser',
        data: { url: 'http://localhost:9000' }
      });

      expect(secondLaunchResponse.success).toBe(true);
      expect(secondLaunchResponse.data.action).toBe('waiting_for_connection');
    });
  });

  describe('Browser Readiness Assurance', () => {
    test('should ensure browser is ready (already connected)', async () => {
      // Connect browser first
      await browserManager.handleMessage({
        type: 'browser_connected',
        data: { connectionId: 'ready-connection' }
      });

      const readyResponse = await browserManager.handleMessage({
        type: 'ensure_browser_ready',
        data: {}
      });

      expect(readyResponse.success).toBe(true);
      expect(readyResponse.data.message).toBe('Browser already ready');
    });

    test('should launch and wait for browser when not ready', async () => {
      const readyResponse = await browserManager.handleMessage({
        type: 'ensure_browser_ready',
        data: {}
      });

      // Should either succeed (if browser connects quickly) or timeout
      expect(typeof readyResponse.success).toBe('boolean');
      
      if (readyResponse.success) {
        expect(readyResponse.data.message).toMatch(/Browser ready|Browser already ready/);
      } else {
        expect(readyResponse.error).toMatch(/timeout/i);
      }
    }, 15000); // Allow 15 seconds for this test
  });

  describe('Connection Health Monitoring', () => {
    test('should provide connection summary', async () => {
      const statusResponse = await browserManager.handleMessage({
        type: 'get_browser_status',
        data: {}
      });

      expect(statusResponse.success).toBe(true);
      expect(statusResponse.data.summary).toBeDefined();
      expect(typeof statusResponse.data.summary.ready).toBe('boolean');
      expect(typeof statusResponse.data.summary.launched).toBe('boolean');
    });

    test('should track connection age', async () => {
      // Connect browser
      await browserManager.handleMessage({
        type: 'browser_connected',
        data: { connectionId: 'age-test-connection' }
      });

      // Wait a bit
      await new Promise(resolve => setTimeout(resolve, 100));

      const statusResponse = await browserManager.handleMessage({
        type: 'get_browser_status',
        data: {}
      });

      expect(statusResponse.data.summary.connectionAge).toBeGreaterThan(50);
    });
  });
});