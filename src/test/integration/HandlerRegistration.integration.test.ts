// ISSUES: 0 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * Handler Registration Integration Tests
 * 
 * Tests the complete handler registration architecture:
 * - Daemon discovery mechanism
 * - Handler registration via WebSocketDaemon
 * - Message routing to registered handlers
 * - Priority-based handler selection
 */

import { test, describe } from 'node:test';
import assert from 'node:assert';
import { SessionManagerDaemon } from '../../daemons/session-manager/SessionManagerDaemon.js';
import { WebSocketDaemon } from '../../integrations/websocket/WebSocketDaemon.js';
import { MESSAGE_HANDLER_REGISTRY } from '../../integrations/websocket/core/MessageHandlerRegistry.js';
import { DAEMON_REGISTRY } from '../../daemons/base/DaemonRegistry.js';

describe('Handler Registration Integration Tests', () => {
  
  describe('Daemon Discovery', () => {
    test('should register and discover daemons', async () => {
      // Create daemons
      const webSocketDaemon = new WebSocketDaemon({ port: 9001 });
      const sessionManagerDaemon = new SessionManagerDaemon();
      
      try {
        // Start daemons
        await webSocketDaemon.start();
        await sessionManagerDaemon.start();
        
        // Verify registration
        const allDaemons = DAEMON_REGISTRY.getAllDaemons();
        const webSocketReg = allDaemons.find(d => d.name === 'websocket-server');
        const sessionReg = allDaemons.find(d => d.name === 'session-manager');
        
        assert.ok(webSocketReg, 'WebSocket daemon should be registered');
        assert.ok(sessionReg, 'SessionManager daemon should be registered');
        assert.strictEqual(webSocketReg.isActive, true);
        assert.strictEqual(sessionReg.isActive, true);
        
      } finally {
        await webSocketDaemon.stop();
        await sessionManagerDaemon.stop();
      }
    });
    
    test('should find daemons by name', async () => {
      const webSocketDaemon = new WebSocketDaemon({ port: 9002 });
      
      try {
        await webSocketDaemon.start();
        
        const found = DAEMON_REGISTRY.findDaemon('websocket-server');
        assert.ok(found, 'Should find WebSocket daemon by name');
        assert.strictEqual(found.name, 'websocket-server');
        
      } finally {
        await webSocketDaemon.stop();
      }
    });
  });
  
  describe('Handler Registration', () => {
    test('should register handlers via WebSocketDaemon', async () => {
      // Clear registry before test
      MESSAGE_HANDLER_REGISTRY.clear();
      
      const webSocketDaemon = new WebSocketDaemon({ port: 9003 });
      const sessionManagerDaemon = new SessionManagerDaemon();
      
      try {
        await webSocketDaemon.start();
        await sessionManagerDaemon.start();
        
        // Register session manager with WebSocket daemon
        webSocketDaemon.registerDaemon(sessionManagerDaemon);
        
        // Verify handler registration
        const hasHandler = MESSAGE_HANDLER_REGISTRY.hasHandlers('send_to_session');
        assert.strictEqual(hasHandler, true, 'send_to_session handler should be registered');
        
        const handlers = MESSAGE_HANDLER_REGISTRY.getHandlers('send_to_session');
        assert.ok(handlers.length >= 1, 'Should have at least one handler');
        
        // Find our specific handler
        const sessionHandler = handlers.find(h => h.priority === 100);
        assert.ok(sessionHandler, 'Should have handler with priority 100');
        
      } finally {
        await webSocketDaemon.stop();
        await sessionManagerDaemon.stop();
        MESSAGE_HANDLER_REGISTRY.clear();
      }
    });
    
    test('should handle message routing to registered handlers', async () => {
      const webSocketDaemon = new WebSocketDaemon({ port: 9004 });
      const sessionManagerDaemon = new SessionManagerDaemon();
      
      try {
        await webSocketDaemon.start();
        await sessionManagerDaemon.start();
        
        // Register session manager
        webSocketDaemon.registerDaemon(sessionManagerDaemon);
        
        // Test message routing
        const testMessage = {
          type: 'send_to_session',
          data: {
            sessionId: 'test-session-123',
            message: { type: 'test', content: 'Hello World' }
          }
        };
        
        const response = await webSocketDaemon.handleMessage(testMessage);
        
        // Should get a response (even if it fails due to no actual session)
        assert.ok(response, 'Should get a response from handler');
        assert.strictEqual(typeof response.success, 'boolean', 'Response should have success field');
        
      } finally {
        await webSocketDaemon.stop();
        await sessionManagerDaemon.stop();
        MESSAGE_HANDLER_REGISTRY.clear();
      }
    });
  });
  
  describe('Handler Priority', () => {
    test('should respect handler priority ordering', async () => {
      // Clear any existing handlers
      MESSAGE_HANDLER_REGISTRY.clear();
      
      // Create mock handlers with different priorities
      const lowPriorityHandler = { priority: 10, handle: async () => ({ success: true, source: 'low' }) };
      const highPriorityHandler = { priority: 100, handle: async () => ({ success: true, source: 'high' }) };
      const noPriorityHandler = { handle: async () => ({ success: true, source: 'none' }) };
      
      // Register in random order
      MESSAGE_HANDLER_REGISTRY.registerHandler('test_message', lowPriorityHandler, 'low-daemon');
      MESSAGE_HANDLER_REGISTRY.registerHandler('test_message', highPriorityHandler, 'high-daemon');
      MESSAGE_HANDLER_REGISTRY.registerHandler('test_message', noPriorityHandler, 'no-priority-daemon');
      
      // Get handlers - should be sorted by priority (highest first)
      const handlers = MESSAGE_HANDLER_REGISTRY.getHandlers('test_message');
      
      assert.strictEqual(handlers.length, 3, 'Should have 3 handlers');
      assert.strictEqual(handlers[0].priority, 100, 'First handler should have highest priority');
      assert.strictEqual(handlers[1].priority, 10, 'Second handler should have medium priority');
      assert.strictEqual(handlers[2].priority || 0, 0, 'Third handler should have default priority');
      
      MESSAGE_HANDLER_REGISTRY.clear();
    });
  });
  
  describe('Handler Cleanup', () => {
    test('should unregister handlers when daemons stop', async () => {
      const webSocketDaemon = new WebSocketDaemon({ port: 9005 });
      const sessionManagerDaemon = new SessionManagerDaemon();
      
      try {
        await webSocketDaemon.start();
        await sessionManagerDaemon.start();
        
        // Register and verify
        webSocketDaemon.registerDaemon(sessionManagerDaemon);
        assert.strictEqual(MESSAGE_HANDLER_REGISTRY.hasHandlers('send_to_session'), true);
        
        // Stop session manager
        await sessionManagerDaemon.stop();
        
        // Verify daemon is unregistered
        const found = DAEMON_REGISTRY.findDaemon('session-manager');
        assert.strictEqual(found, null, 'Session manager should be unregistered');
        
      } finally {
        await webSocketDaemon.stop();
        MESSAGE_HANDLER_REGISTRY.clear();
      }
    });
  });
  
  describe('Real-world Integration', () => {
    test('should support the complete handler registration flow', async () => {
      const webSocketDaemon = new WebSocketDaemon({ port: 9006 });
      const sessionManagerDaemon = new SessionManagerDaemon();
      
      try {
        // 1. Start daemons (simulates system startup)
        await webSocketDaemon.start();
        await sessionManagerDaemon.start();
        
        // 2. Register daemons (simulates ContinuumSystemStartup)
        webSocketDaemon.registerDaemon(sessionManagerDaemon);
        
        // 3. Verify complete integration
        const registered = MESSAGE_HANDLER_REGISTRY.getRegisteredTypes();
        assert.ok(registered.includes('send_to_session'), 'Handler should be registered');
        
        // 4. Test message handling
        const message = {
          type: 'send_to_session',
          data: {
            sessionId: 'integration-test-session',
            message: { type: 'ping', timestamp: Date.now() }
          }
        };
        
        const response = await webSocketDaemon.handleMessage(message);
        assert.ok(response, 'Should handle message');
        
        // 5. Verify handler registry status
        const handlerCount = MESSAGE_HANDLER_REGISTRY.getHandlerCount('send_to_session');
        assert.ok(handlerCount >= 1, 'Should have at least one handler registered');
        
      } finally {
        await webSocketDaemon.stop();
        await sessionManagerDaemon.stop();
        MESSAGE_HANDLER_REGISTRY.clear();
      }
    });
  });
});