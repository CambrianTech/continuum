// ISSUES: 0 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * Message Handler Registry Unit Tests
 * 
 * Tests duplicate prevention and handler management logic
 */

import { test, describe } from 'node:test';
import assert from 'node:assert';
import { DefaultMessageHandlerRegistry } from '../../integrations/websocket/core/MessageHandlerRegistry.js';
import { MessageHandler } from '../../integrations/websocket/types/MessageHandler.js';

describe('MessageHandlerRegistry Unit Tests', () => {
  
  describe('Duplicate Prevention', () => {
    test('should replace duplicate handlers by default', () => {
      const registry = new DefaultMessageHandlerRegistry();
      
      const handler1: MessageHandler = { 
        handle: async () => ({ success: true, source: 'handler1' }),
        priority: 50
      };
      
      const handler2: MessageHandler = { 
        handle: async () => ({ success: true, source: 'handler2' }),
        priority: 75
      };
      
      // Register first handler
      registry.registerHandler('test_message', handler1, 'test-daemon');
      assert.strictEqual(registry.getHandlerCount('test_message'), 1);
      
      // Register second handler from same daemon - should replace
      registry.registerHandler('test_message', handler2, 'test-daemon');
      assert.strictEqual(registry.getHandlerCount('test_message'), 1);
      
      // Verify it's the new handler (priority 75)
      const handlers = registry.getHandlers('test_message');
      assert.strictEqual(handlers[0].priority, 75);
      
      registry.clear();
    });
    
    test('should throw error when allowReplace is false', () => {
      const registry = new DefaultMessageHandlerRegistry();
      
      const handler1: MessageHandler = { 
        handle: async () => ({ success: true }),
        priority: 50
      };
      
      const handler2: MessageHandler = { 
        handle: async () => ({ success: true }),
        priority: 75
      };
      
      // Register first handler
      registry.registerHandler('test_message', handler1, 'test-daemon');
      
      // Try to register duplicate with allowReplace: false
      assert.throws(() => {
        registry.registerHandler('test_message', handler2, 'test-daemon', { allowReplace: false });
      }, /Handler for 'test_message' from daemon 'test-daemon' already exists/);
      
      registry.clear();
    });
    
    test('should allow multiple handlers from different daemons', () => {
      const registry = new DefaultMessageHandlerRegistry();
      
      const handler1: MessageHandler = { 
        handle: async () => ({ success: true }),
        priority: 50
      };
      
      const handler2: MessageHandler = { 
        handle: async () => ({ success: true }),
        priority: 75
      };
      
      // Register handlers from different daemons
      registry.registerHandler('test_message', handler1, 'daemon-1');
      registry.registerHandler('test_message', handler2, 'daemon-2');
      
      assert.strictEqual(registry.getHandlerCount('test_message'), 2);
      
      // Should be sorted by priority (highest first)
      const handlers = registry.getHandlers('test_message');
      assert.strictEqual(handlers[0].priority, 75); // daemon-2
      assert.strictEqual(handlers[1].priority, 50); // daemon-1
      
      registry.clear();
    });
    
    test('should handle mixed priority scenarios correctly', () => {
      const registry = new DefaultMessageHandlerRegistry();
      
      const lowHandler: MessageHandler = { 
        handle: async () => ({ success: true }),
        priority: 10
      };
      
      const highHandler: MessageHandler = { 
        handle: async () => ({ success: true }),
        priority: 100
      };
      
      const mediumHandler: MessageHandler = { 
        handle: async () => ({ success: true }),
        priority: 50
      };
      
      // Register in random order
      registry.registerHandler('test_message', mediumHandler, 'daemon-medium');
      registry.registerHandler('test_message', lowHandler, 'daemon-low');  
      registry.registerHandler('test_message', highHandler, 'daemon-high');
      
      const handlers = registry.getHandlers('test_message');
      assert.strictEqual(handlers.length, 3);
      
      // Should be sorted: high (100), medium (50), low (10)
      assert.strictEqual(handlers[0].priority, 100);
      assert.strictEqual(handlers[1].priority, 50);
      assert.strictEqual(handlers[2].priority, 10);
      
      registry.clear();
    });
  });
  
  describe('Registry Management', () => {
    test('should provide accurate handler counts', () => {
      const registry = new DefaultMessageHandlerRegistry();
      
      const handler: MessageHandler = { 
        handle: async () => ({ success: true })
      };
      
      assert.strictEqual(registry.getHandlerCount('nonexistent'), 0);
      
      registry.registerHandler('test_message', handler, 'daemon-1');
      assert.strictEqual(registry.getHandlerCount('test_message'), 1);
      
      registry.registerHandler('test_message', handler, 'daemon-2');
      assert.strictEqual(registry.getHandlerCount('test_message'), 2);
      
      registry.clear();
      assert.strictEqual(registry.getHandlerCount('test_message'), 0);
    });
    
    test('should list registered message types', () => {
      const registry = new DefaultMessageHandlerRegistry();
      
      const handler: MessageHandler = { 
        handle: async () => ({ success: true })
      };
      
      assert.deepStrictEqual(registry.getRegisteredTypes(), []);
      
      registry.registerHandler('type_a', handler, 'daemon-1');
      registry.registerHandler('type_b', handler, 'daemon-2');
      
      const types = registry.getRegisteredTypes().sort();
      assert.deepStrictEqual(types, ['type_a', 'type_b']);
      
      registry.clear();
    });
    
    test('should provide registration info for debugging', () => {
      const registry = new DefaultMessageHandlerRegistry();
      
      const handler: MessageHandler = { 
        handle: async () => ({ success: true }),
        priority: 42
      };
      
      registry.registerHandler('debug_message', handler, 'debug-daemon');
      
      const registrations = registry.getRegistrationInfo();
      assert.strictEqual(registrations.length, 1);
      assert.strictEqual(registrations[0].messageType, 'debug_message');
      assert.strictEqual(registrations[0].daemonName, 'debug-daemon');
      assert.strictEqual(registrations[0].handler.priority, 42);
      assert.ok(registrations[0].registeredAt instanceof Date);
      
      registry.clear();
    });
  });
});