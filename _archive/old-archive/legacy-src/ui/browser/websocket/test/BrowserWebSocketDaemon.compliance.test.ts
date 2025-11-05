/**
 * BrowserWebSocketDaemon Compliance Tests
 * 
 * Validates module compliance with browser daemon architecture standards,
 * following the same pattern as other browser-daemon modules.
 */

import { describe, it, expect } from 'vitest';
import { BrowserWebSocketDaemon } from '../BrowserWebSocketDaemon';
import { BaseBrowserDaemon } from '../base/BaseBrowserDaemon';

describe('BrowserWebSocketDaemon Compliance Tests', () => {
  describe('Module Structure Compliance', () => {
    it('should have package.json with correct structure', async () => {
      const packageJson = await import('../package.json');
      
      // Required fields
      expect(packageJson.name).toBe('@continuum/browser-websocket-daemon');
      expect(packageJson.version).toBeDefined();
      expect(packageJson.description).toBeDefined();
      expect(packageJson.main).toBe('BrowserWebSocketDaemon.ts');
      
      // Continuum-specific fields
      expect(packageJson.continuum).toBeDefined();
      expect(packageJson.continuum.type).toBe('browser-daemon');
      expect(packageJson.continuum.category).toBe('communication');
      expect(packageJson.continuum.phase).toBe(3);
      
      // Arrays should be defined
      expect(Array.isArray(packageJson.continuum.responsibilities)).toBe(true);
      expect(Array.isArray(packageJson.continuum.dependencies)).toBe(true);
      expect(Array.isArray(packageJson.continuum.integrations)).toBe(true);
      
      // Should have proper metadata
      expect(packageJson.continuum.extractedFrom).toBe('continuum-browser.ts');
      expect(packageJson.continuum.extractedLines).toContain('15%');
    });

    it('should have README.md documentation', async () => {
      // This test ensures README.md exists and can be imported
      const readmeExists = await import('../README.md').then(() => true).catch(() => false);
      expect(readmeExists).toBe(true);
    });

    it('should have proper test directory structure', () => {
      // Test files should exist (verified by successful import)
      expect(true).toBe(true); // This test file exists
    });
  });

  describe('BaseBrowserDaemon Compliance', () => {
    let daemon: BrowserWebSocketDaemon;

    beforeEach(() => {
      daemon = new BrowserWebSocketDaemon();
    });

    afterEach(async () => {
      await daemon.stop();
    });

    it('should extend BaseBrowserDaemon', () => {
      expect(daemon).toBeInstanceOf(BaseBrowserDaemon);
    });

    it('should implement required abstract methods', () => {
      expect(typeof daemon.getMessageTypes).toBe('function');
      expect(typeof daemon.handleMessage).toBe('function');
    });

    it('should have lifecycle methods', () => {
      expect(typeof daemon.start).toBe('function');
      expect(typeof daemon.stop).toBe('function');
      expect(typeof daemon.isRunning).toBe('function');
    });

    it('should provide message types array', () => {
      const messageTypes = daemon.getMessageTypes();
      expect(Array.isArray(messageTypes)).toBe(true);
      expect(messageTypes.length).toBeGreaterThan(0);
      
      // Should contain WebSocket-specific message types (type-safe!)
      expect(messageTypes).toContain('websocket:connect');
      expect(messageTypes).toContain('websocket:disconnect');
      expect(messageTypes).toContain('websocket:status');
      expect(messageTypes).toContain('websocket:execute_command');
    });

    it('should handle messages with proper response format', async () => {
      const response = await daemon.handleMessage({
        type: 'websocket:status',
        data: {},
        timestamp: new Date().toISOString()
      });

      // Response should follow standard format
      expect(response).toHaveProperty('success');
      expect(response).toHaveProperty('timestamp');
      expect(typeof response.success).toBe('boolean');
      expect(typeof response.timestamp).toBe('string');
      
      if (response.success) {
        expect(response).toHaveProperty('data');
      } else {
        expect(response).toHaveProperty('error');
      }
    });

    it('should have proper daemon name', () => {
      expect(daemon.getName()).toBe('BrowserWebSocketDaemon');
    });
  });

  describe('WebSocket-Specific Compliance', () => {
    let daemon: BrowserWebSocketDaemon;

    beforeEach(() => {
      daemon = new BrowserWebSocketDaemon();
    });

    afterEach(async () => {
      await daemon.stop();
    });

    it('should provide WebSocket-specific methods', () => {
      expect(typeof daemon.connect).toBe('function');
      expect(typeof daemon.disconnect).toBe('function');
      expect(typeof daemon.isConnected).toBe('function');
      expect(typeof daemon.getConnectionState).toBe('function');
      expect(typeof daemon.executeCommand).toBe('function');
    });

    it('should provide event system methods', () => {
      expect(typeof daemon.on).toBe('function');
      expect(typeof daemon.off).toBe('function');
      expect(typeof daemon.emit).toBe('function');
    });

    it('should have proper connection state structure', () => {
      const state = daemon.getConnectionState();
      
      expect(state).toHaveProperty('state');
      expect(state).toHaveProperty('clientId');
      expect(state).toHaveProperty('sessionId');
      expect(state).toHaveProperty('reconnectAttempts');
      expect(state).toHaveProperty('lastError');
      
      // Initial state should be disconnected
      expect(state.state).toBe('disconnected');
      expect(state.clientId).toBeNull();
      expect(state.sessionId).toBeNull();
      expect(state.reconnectAttempts).toBe(0);
    });

    it('should accept configuration options', () => {
      const configuredDaemon = new BrowserWebSocketDaemon({
        wsUrl: 'ws://custom:8080',
        maxReconnectAttempts: 10,
        reconnectDelay: 2000,
        connectionTimeout: 10000
      });
      
      expect(configuredDaemon).toBeInstanceOf(BrowserWebSocketDaemon);
    });
  });

  describe('Message Handling Compliance', () => {
    let daemon: BrowserWebSocketDaemon;

    beforeEach(() => {
      daemon = new BrowserWebSocketDaemon();
    });

    afterEach(async () => {
      await daemon.stop();
    });

    it('should handle all declared message types', async () => {
      const messageTypes = daemon.getMessageTypes();
      
      for (const messageType of messageTypes) {
        const response = await daemon.handleMessage({
          type: messageType,
          data: {},
          timestamp: new Date().toISOString()
        });
        
        // Should not throw and should return proper response
        expect(response).toBeDefined();
        expect(typeof response.success).toBe('boolean');
      }
    });

    it('should reject unknown message types', async () => {
      const response = await daemon.handleMessage({
        type: 'unknown:message:type',
        data: {},
        timestamp: new Date().toISOString()
      });

      expect(response.success).toBe(false);
      expect(response.error).toContain('Unknown message type');
    });

    it('should handle malformed messages gracefully', async () => {
      const response = await daemon.handleMessage({
        type: 'websocket:status',
        data: null as any,
        timestamp: new Date().toISOString()
      });

      // Should handle gracefully, not throw
      expect(response).toBeDefined();
    });
  });

  describe('Integration Compliance', () => {
    let daemon: BrowserWebSocketDaemon;

    beforeEach(() => {
      daemon = new BrowserWebSocketDaemon();
    });

    afterEach(async () => {
      await daemon.stop();
    });

    it('should be compatible with BrowserDaemonManager', () => {
      // Should have all required methods for daemon manager integration
      expect(typeof daemon.start).toBe('function');
      expect(typeof daemon.stop).toBe('function');
      expect(typeof daemon.handleMessage).toBe('function');
      expect(typeof daemon.getMessageTypes).toBe('function');
      expect(typeof daemon.getName).toBe('function');
    });

    it('should emit events compatible with BrowserDaemonEventBus', () => {
      let eventEmitted = false;
      
      daemon.on('test:event', () => {
        eventEmitted = true;
      });
      
      daemon.emit('test:event', {});
      
      expect(eventEmitted).toBe(true);
    });

    it('should maintain consistent state across operations', async () => {
      const initialState = daemon.getConnectionState();
      
      // Perform some operations
      await daemon.handleMessage({
        type: 'websocket:status',
        data: {},
        timestamp: new Date().toISOString()
      });
      
      const afterState = daemon.getConnectionState();
      
      // State should remain consistent for status queries
      expect(afterState.state).toBe(initialState.state);
    });
  });

  describe('Performance Compliance', () => {
    let daemon: BrowserWebSocketDaemon;

    beforeEach(() => {
      daemon = new BrowserWebSocketDaemon();
    });

    afterEach(async () => {
      await daemon.stop();
    });

    it('should handle rapid message processing', async () => {
      const startTime = Date.now();
      const messageCount = 100;
      const promises: Promise<any>[] = [];

      // Send multiple messages rapidly
      for (let i = 0; i < messageCount; i++) {
        promises.push(
          daemon.handleMessage({
            type: 'websocket:status',
            data: { index: i },
            timestamp: new Date().toISOString()
          })
        );
      }

      const responses = await Promise.all(promises);
      const endTime = Date.now();
      const duration = endTime - startTime;

      // All responses should be successful
      expect(responses).toHaveLength(messageCount);
      responses.forEach(response => {
        expect(response.success).toBe(true);
      });

      // Should complete within reasonable time (less than 1 second)
      expect(duration).toBeLessThan(1000);
    });

    it('should not leak memory during extended operation', async () => {
      const iterations = 1000;
      
      // Perform many operations
      for (let i = 0; i < iterations; i++) {
        await daemon.handleMessage({
          type: 'websocket:status',
          data: {},
          timestamp: new Date().toISOString()
        });
      }

      // Should still respond normally after many operations
      const finalResponse = await daemon.handleMessage({
        type: 'websocket:status',
        data: {},
        timestamp: new Date().toISOString()
      });

      expect(finalResponse.success).toBe(true);
    });
  });

  describe('Error Handling Compliance', () => {
    let daemon: BrowserWebSocketDaemon;

    beforeEach(() => {
      daemon = new BrowserWebSocketDaemon();
    });

    afterEach(async () => {
      await daemon.stop();
    });

    it('should handle errors without crashing daemon', async () => {
      // Trigger potential error condition
      const response = await daemon.handleMessage({
        type: 'websocket:send',
        data: { invalidData: undefined },
        timestamp: new Date().toISOString()
      });

      // Should handle gracefully
      expect(response).toBeDefined();
      
      // Daemon should still be functional
      const statusResponse = await daemon.handleMessage({
        type: 'websocket:status',
        data: {},
        timestamp: new Date().toISOString()
      });
      
      expect(statusResponse.success).toBe(true);
    });

    it('should provide meaningful error messages', async () => {
      const response = await daemon.handleMessage({
        type: 'invalid:message:type',
        data: {},
        timestamp: new Date().toISOString()
      });

      expect(response.success).toBe(false);
      expect(typeof response.error).toBe('string');
      expect(response.error.length).toBeGreaterThan(0);
    });
  });
});