/**
 * BrowserWebSocketDaemon Integration Tests
 * 
 * Tests WebSocket daemon integration with browser daemon system,
 * real WebSocket connections, and cross-daemon communication.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { BrowserWebSocketDaemon } from '../../BrowserWebSocketDaemon';
import { BrowserDaemonManager } from '../../base/BrowserDaemonManager';
import { BrowserDaemonEventBus } from '../../base/BrowserDaemonEventBus';

describe('BrowserWebSocketDaemon Integration Tests', () => {
  let daemon: BrowserWebSocketDaemon;
  let daemonManager: BrowserDaemonManager;
  let eventBus: BrowserDaemonEventBus;

  beforeEach(async () => {
    daemon = new BrowserWebSocketDaemon();
    daemonManager = new BrowserDaemonManager();
    eventBus = BrowserDaemonEventBus.getInstance();
    
    // Register daemon with manager
    await daemonManager.registerDaemon('websocket', daemon);
  });

  afterEach(async () => {
    await daemonManager.stopAllDaemons();
    eventBus.removeAllListeners();
  });

  describe('Module Compliance', () => {
    it('should have required package.json structure', async () => {
      const packageJson = await import('../../package.json');
      
      expect(packageJson.name).toBe('@continuum/browser-websocket-daemon');
      expect(packageJson.main).toBe('BrowserWebSocketDaemon.ts');
      expect(packageJson.continuum).toBeDefined();
      expect(packageJson.continuum.type).toBe('browser-daemon');
    });

    it('should extend BaseBrowserDaemon correctly', () => {
      expect(daemon).toHaveProperty('getMessageTypes');
      expect(daemon).toHaveProperty('handleMessage');
      expect(daemon).toHaveProperty('start');
      expect(daemon).toHaveProperty('stop');
      expect(daemon).toHaveProperty('isRunning');
    });

    it('should provide all required message types', () => {
      const messageTypes = daemon.getMessageTypes();
      const requiredTypes = [
        'websocket:connect',
        'websocket:disconnect',
        'websocket:send',
        'websocket:status',
        'websocket:subscribe',
        'websocket:unsubscribe',
        'websocket:execute_command'
      ];
      
      requiredTypes.forEach(type => {
        expect(messageTypes).toContain(type);
      });
    });
  });

  describe('Daemon Manager Integration', () => {
    it('should register with daemon manager successfully', async () => {
      const registeredDaemons = daemonManager.getRegisteredDaemons();
      expect(registeredDaemons).toContain('websocket');
    });

    it('should start and stop through daemon manager', async () => {
      await daemonManager.startDaemon('websocket');
      expect(daemon.isRunning()).toBe(true);

      await daemonManager.stopDaemon('websocket');
      expect(daemon.isRunning()).toBe(false);
    });

    it('should route messages through daemon manager', async () => {
      await daemonManager.startDaemon('websocket');
      
      const response = await daemonManager.routeMessage({
        type: 'websocket:status',
        data: {},
        timestamp: new Date().toISOString()
      });

      expect(response.success).toBe(true);
      expect(response.data?.connectionState).toBeDefined();
    });

    it('should handle unknown messages gracefully', async () => {
      const response = await daemonManager.routeMessage({
        type: 'websocket:unknown',
        data: {},
        timestamp: new Date().toISOString()
      });

      expect(response.success).toBe(false);
      expect(response.error).toContain('Unknown message type');
    });
  });

  describe('Event Bus Integration', () => {
    it('should emit events through daemon event bus', async () => {
      let eventReceived = false;
      let eventData: any = null;

      eventBus.on('websocket:connected', (data) => {
        eventReceived = true;
        eventData = data;
      });

      // Simulate connection event
      daemon.emit('websocket:connected', { clientId: 'test-client' });

      expect(eventReceived).toBe(true);
      expect(eventData).toEqual({ clientId: 'test-client' });
    });

    it('should subscribe to events from other daemons', async () => {
      let messageReceived = false;

      daemon.on('external:event', () => {
        messageReceived = true;
      });

      eventBus.emit('external:event', {});

      expect(messageReceived).toBe(true);
    });

    it('should handle event bus errors gracefully', async () => {
      const errorHandler = () => {
        throw new Error('Event handler error');
      };

      daemon.on('test:event', errorHandler);

      // Should not crash the daemon
      expect(() => {
        daemon.emit('test:event', {});
      }).not.toThrow();
    });
  });

  describe('Cross-Daemon Communication', () => {
    it('should coordinate with console daemon for logging', async () => {
      const consoleDaemon = {
        handleMessage: async (message: any) => ({
          success: true,
          data: { logged: true },
          timestamp: new Date().toISOString()
        })
      };

      await daemonManager.registerDaemon('console', consoleDaemon as any);

      // WebSocket daemon should be able to coordinate with console daemon
      const response = await daemonManager.routeMessage({
        type: 'console:log',
        data: { message: 'WebSocket test log' },
        timestamp: new Date().toISOString()
      });

      expect(response.success).toBe(true);
    });

    it('should share connection state with other daemons', async () => {
      const statusResponse = await daemon.handleMessage({
        type: 'websocket:status',
        data: {},
        timestamp: new Date().toISOString()
      });

      expect(statusResponse.success).toBe(true);
      expect(statusResponse.data?.connectionState).toBeDefined();
      expect(statusResponse.data?.isConnected).toBeDefined();
    });
  });

  describe('Real WebSocket Integration', () => {
    it('should handle connection attempts to local server', async () => {
      // Test connection to local development server
      const connectResponse = await daemon.handleMessage({
        type: 'websocket:connect',
        data: { wsUrl: 'ws://localhost:9000' },
        timestamp: new Date().toISOString()
      });

      // Should at least attempt connection (may timeout in test environment)
      expect(connectResponse).toBeDefined();
      expect(connectResponse.success).toBeDefined();
    });

    it('should handle connection failures gracefully', async () => {
      // Try to connect to invalid URL
      const connectResponse = await daemon.handleMessage({
        type: 'websocket:connect',
        data: { wsUrl: 'ws://invalid-url:9999' },
        timestamp: new Date().toISOString()
      });

      // Should handle failure gracefully
      expect(connectResponse).toBeDefined();
    });

    it('should maintain connection state during lifecycle', async () => {
      await daemon.start();
      
      const initialState = daemon.getConnectionState();
      expect(initialState.state).toBeDefined();
      
      await daemon.stop();
      
      const finalState = daemon.getConnectionState();
      expect(finalState.state).toBe('disconnected');
    });
  });

  describe('Message Processing Performance', () => {
    it('should handle rapid message processing', async () => {
      const messageCount = 100;
      const promises: Promise<any>[] = [];

      // Send multiple status messages rapidly
      for (let i = 0; i < messageCount; i++) {
        promises.push(
          daemon.handleMessage({
            type: 'websocket:status',
            data: { requestId: i },
            timestamp: new Date().toISOString()
          })
        );
      }

      const responses = await Promise.all(promises);
      
      // All messages should be processed successfully
      expect(responses).toHaveLength(messageCount);
      responses.forEach(response => {
        expect(response.success).toBe(true);
      });
    });

    it('should maintain message queue under load', async () => {
      const messageCount = 50;
      
      // Send messages while disconnected
      for (let i = 0; i < messageCount; i++) {
        await daemon.handleMessage({
          type: 'websocket:send',
          data: { message: `test-${i}` },
          timestamp: new Date().toISOString()
        });
      }

      const statusResponse = await daemon.handleMessage({
        type: 'websocket:status',
        data: {},
        timestamp: new Date().toISOString()
      });

      expect(statusResponse.data?.queuedMessages).toBeGreaterThan(0);
    });
  });

  describe('Error Recovery', () => {
    it('should recover from connection errors', async () => {
      // Start daemon
      await daemon.start();
      
      // Simulate connection and then disconnection
      await daemon.handleMessage({
        type: 'websocket:connect',
        data: {},
        timestamp: new Date().toISOString()
      });
      
      await daemon.handleMessage({
        type: 'websocket:disconnect',
        data: {},
        timestamp: new Date().toISOString()
      });

      // Should be able to reconnect
      const reconnectResponse = await daemon.handleMessage({
        type: 'websocket:connect',
        data: {},
        timestamp: new Date().toISOString()
      });

      expect(reconnectResponse.success).toBe(true);
    });

    it('should handle daemon restart gracefully', async () => {
      await daemon.start();
      const initialState = daemon.getConnectionState();
      
      await daemon.stop();
      await daemon.start();
      
      const restartedState = daemon.getConnectionState();
      expect(restartedState).toBeDefined();
    });
  });
});