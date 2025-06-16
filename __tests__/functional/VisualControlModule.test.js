/**
 * Visual Control Module Functional Tests
 * Tests the visual control module integration and WebSocket communication
 */

import { test, describe } from 'node:test';
import assert from 'node:assert';
import { createRequire } from 'module';
import { EventEmitter } from 'events';

const require = createRequire(import.meta.url);

describe('Visual Control Module Functional Tests', () => {

  test('should load visual control module correctly', async () => {
    // Test that the visual control module can be imported
    const { VisualControlModule, createVisualControlModule } = require('../../visual-control-module.cjs');
    
    assert(VisualControlModule);
    assert(typeof createVisualControlModule === 'function');
    
    // Test factory function
    const visualAI = createVisualControlModule('TestVisualAI');
    assert(visualAI instanceof VisualControlModule);
    assert.strictEqual(visualAI.agentName, 'TestVisualAI');
  });

  test('should have correct configuration and properties', () => {
    const { VisualControlModule } = require('../../visual-control-module.cjs');
    const module = new VisualControlModule({ agentName: 'TestAgent' });
    
    assert.strictEqual(module.agentName, 'TestAgent');
    assert.strictEqual(module.isConnected, false);
    assert.strictEqual(module.sessionId, null);
    assert(Array.isArray(module.messageQueue));
    assert.strictEqual(module.isProcessing, false);
  });

  test('should handle WebSocket connection simulation', async () => {
    const { VisualControlModule } = require('../../visual-control-module.cjs');
    const module = new VisualControlModule({ agentName: 'TestAgent' });
    
    // Mock WebSocket for testing
    class MockWebSocket extends EventEmitter {
      constructor() {
        super();
        this.readyState = 1; // OPEN
      }
      
      send(data) {
        this.lastSent = data;
      }
      
      close() {
        this.emit('close');
      }
    }
    
    // Simulate connection
    const mockWS = new MockWebSocket();
    module.ws = mockWS;
    module.isConnected = true;
    
    // Test message sending
    module.sendMessage('Test message');
    
    const sentData = JSON.parse(mockWS.lastSent);
    assert.strictEqual(sentData.type, 'userMessage');
    assert.strictEqual(sentData.message, 'Test message');
  });

  test('should handle message parsing correctly', () => {
    const { VisualControlModule } = require('../../visual-control-module.cjs');
    const module = new VisualControlModule({ agentName: 'TestAgent' });
    
    // Test status message handling
    const statusMessage = JSON.stringify({
      type: 'status',
      data: { sessionId: 'test-session-123' }
    });
    
    module.handleMessage(statusMessage);
    assert.strictEqual(module.sessionId, 'test-session-123');
    
    // Test result message handling (should not throw)
    const resultMessage = JSON.stringify({
      type: 'result',
      data: { result: 'Test result data for visual control system verification' }
    });
    
    assert.doesNotThrow(() => {
      module.handleMessage(resultMessage);
    });
    
    // Test invalid message handling (should not throw)
    assert.doesNotThrow(() => {
      module.handleMessage('invalid json');
    });
  });

  test('should have screenshot request functionality', () => {
    const { VisualControlModule } = require('../../visual-control-module.cjs');
    const module = new VisualControlModule({ agentName: 'TestAgent' });
    
    // Mock connection state
    module.isConnected = true;
    const sentMessages = [];
    module.ws = {
      send: (data) => sentMessages.push(JSON.parse(data))
    };
    
    // Test screenshot request without options
    module.requestScreenshot();
    assert.strictEqual(sentMessages.length, 1);
    assert(sentMessages[0].message.includes('screenshot'));
    
    // Test screenshot request with options
    module.requestScreenshot('low resolution');
    assert.strictEqual(sentMessages.length, 2);
    assert(sentMessages[1].message.includes('low resolution'));
  });

  test('should have cursor control functionality', () => {
    const { VisualControlModule } = require('../../visual-control-module.cjs');
    const module = new VisualControlModule({ agentName: 'TestAgent' });
    
    // Mock connection state
    module.isConnected = true;
    const sentMessages = [];
    module.ws = {
      send: (data) => sentMessages.push(JSON.parse(data))
    };
    
    // Test cursor position request
    module.requestCursorPosition();
    assert.strictEqual(sentMessages.length, 1);
    assert(sentMessages[0].message.includes('cursor position'));
    
    // Test cursor move request
    module.requestCursorMove(400, 300);
    assert.strictEqual(sentMessages.length, 2);
    assert(sentMessages[1].message.includes('400'));
    assert(sentMessages[1].message.includes('300'));
    
    // Test AI cursor activation
    module.requestAICursor();
    assert.strictEqual(sentMessages.length, 3);
    assert(sentMessages[2].message.includes('AI cursor'));
  });

  test('should handle interface analysis requests', () => {
    const { VisualControlModule } = require('../../visual-control-module.cjs');
    const module = new VisualControlModule({ agentName: 'TestAgent' });
    
    // Mock connection state
    module.isConnected = true;
    const sentMessages = [];
    module.ws = {
      send: (data) => sentMessages.push(JSON.parse(data))
    };
    
    // Test interface analysis
    module.analyzeInterface();
    assert.strictEqual(sentMessages.length, 1);
    assert(sentMessages[0].message.includes('analyze'));
    assert(sentMessages[0].message.includes('interface'));
  });

  test('should support capability demonstration workflow', async () => {
    const { VisualControlModule } = require('../../visual-control-module.cjs');
    const module = new VisualControlModule({ agentName: 'DemoAgent' });
    
    // Mock connection and timing for demonstration
    module.isConnected = true;
    const sentMessages = [];
    module.ws = {
      send: (data) => sentMessages.push(JSON.parse(data))
    };
    
    // Override wait function for testing
    module.wait = async (ms) => {
      return Promise.resolve();
    };
    
    // Run demonstration
    await module.demonstrateCapabilities();
    
    // Verify demonstration steps were executed
    assert(sentMessages.length >= 5); // Should have multiple demo steps
    
    // Verify key demonstration messages
    const messages = sentMessages.map(msg => msg.message);
    assert(messages.some(msg => msg.includes('DemoAgent')));
    assert(messages.some(msg => msg.includes('screenshot')));
    assert(messages.some(msg => msg.includes('analyze')));
    assert(messages.some(msg => msg.includes('cursor')));
  });

  test('should handle disconnection gracefully', () => {
    const { VisualControlModule } = require('../../visual-control-module.cjs');
    const module = new VisualControlModule({ agentName: 'TestAgent' });
    
    // Set connected state
    module.isConnected = true;
    module.ws = {
      close: () => {}
    };
    
    // Test disconnect
    assert.doesNotThrow(() => {
      module.disconnect();
    });
  });

  test('should handle missing connection state correctly', () => {
    const { VisualControlModule } = require('../../visual-control-module.cjs');
    const module = new VisualControlModule({ agentName: 'TestAgent' });
    
    // Test sending message when not connected
    assert.doesNotThrow(() => {
      module.sendMessage('Test message when disconnected');
    });
    
    // Should not crash when ws is null
    module.ws = null;
    assert.doesNotThrow(() => {
      module.disconnect();
    });
  });

  test('should integrate with continuum command protocol', () => {
    const { VisualControlModule } = require('../../visual-control-module.cjs');
    const module = new VisualControlModule({ agentName: 'IntegrationAgent' });
    
    // Mock connection
    module.isConnected = true;
    const sentMessages = [];
    module.ws = {
      send: (data) => sentMessages.push(JSON.parse(data))
    };
    
    // Test introduction message
    module.introduce();
    assert.strictEqual(sentMessages.length, 1);
    
    const introMessage = sentMessages[0];
    assert.strictEqual(introMessage.type, 'userMessage');
    assert(introMessage.message.includes('IntegrationAgent'));
    assert(introMessage.message.includes('visual control AI module'));
    assert(introMessage.message.includes('standard Continuum commands'));
  });

  test('should validate agent name configuration', () => {
    const { VisualControlModule } = require('../../visual-control-module.cjs');
    
    // Test default agent name
    const defaultModule = new VisualControlModule();
    assert.strictEqual(defaultModule.agentName, 'VisualAI');
    
    // Test custom agent name
    const customModule = new VisualControlModule({ agentName: 'CustomVisualAgent' });
    assert.strictEqual(customModule.agentName, 'CustomVisualAgent');
  });

  test('should handle message queue functionality', () => {
    const { VisualControlModule } = require('../../visual-control-module.cjs');
    const module = new VisualControlModule({ agentName: 'QueueAgent' });
    
    // Test initial queue state
    assert(Array.isArray(module.messageQueue));
    assert.strictEqual(module.messageQueue.length, 0);
    assert.strictEqual(module.isProcessing, false);
    
    // Message queue is available for future implementation
    assert(module.hasOwnProperty('messageQueue'));
    assert(module.hasOwnProperty('isProcessing'));
  });
});