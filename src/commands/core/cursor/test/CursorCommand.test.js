/**
 * Cursor Command Tests
 * Tests continuon cursor activation, deactivation, and positioning
 */

import { test, describe } from 'node:test';
import assert from 'node:assert';
import { createRequire } from 'module';

const require = createRequire(import.meta.url);
const CursorCommand = require('../CursorCommand.cjs');

describe('Cursor Command', () => {

  function createTestContext() {
    return {
      command: new CursorCommand(),
      mockContext: {
        webSocketServer: {
          broadcast: (message) => {
            this.lastBroadcast = message;
          }
        },
        lastBroadcast: null
      }
    };
  }

  test('should be properly configured', () => {
    const { command } = createTestContext();
    assert.strictEqual(command.name, 'cursor');
    assert.strictEqual(command.description, 'Control continuon (AI cursor) activation, deactivation, and positioning');
    assert.strictEqual(command.icon, '🟢');
    assert.strictEqual(command.category, 'continuon control');
  });

  test('should have correct parameters', () => {
    const { command } = createTestContext();
    const definition = command.getDefinition();
    assert(definition.parameters.action);
    assert(definition.parameters.action.required);
    assert.strictEqual(definition.parameters.action.type, 'string');
  });

  test('should activate cursor with websocket context', async () => {
    const { command, mockContext } = createTestContext();
    const result = await command.execute('{"action": "activate"}', mockContext);
    
    assert(result.success);
    assert(result.data.activated);
    assert(result.data.message.includes('activated'));
  });

  test('should fail activation without websocket context', async () => {
    const { command } = createTestContext();
    const result = await command.execute('{"action": "activate"}', null);
    
    assert(!result.success);
    assert(result.error.includes('No browser connection'));
  });

  test('should deactivate cursor with websocket context', async () => {
    const { command, mockContext } = createTestContext();
    const result = await command.execute('{"action": "deactivate"}', mockContext);
    
    assert(result.success);
    assert(result.data.deactivated);
    assert(result.data.message.includes('deactivated'));
  });

  test('should position cursor at specified coordinates', async () => {
    const { command, mockContext } = createTestContext();
    const result = await command.execute('{"action": "position", "x": 400, "y": 300}', mockContext);
    
    assert(result.success);
    assert(result.data.positioned);
    assert.deepStrictEqual(result.data.position, { x: 400, y: 300 });
  });

  test('should fail positioning without coordinates', async () => {
    const { command, mockContext } = createTestContext();
    const result = await command.execute('{"action": "position"}', mockContext);
    
    assert(!result.success);
    assert(result.error.includes('coordinates required'));
  });

  test('should fail positioning with invalid coordinates', async () => {
    const { command, mockContext } = createTestContext();
    const result = await command.execute('{"action": "position", "x": "invalid", "y": 300}', mockContext);
    
    assert(!result.success);
    assert(result.error.includes('Invalid coordinates'));
  });

  test('should request cursor status', async () => {
    const { command, mockContext } = createTestContext();
    const result = await command.execute('{"action": "status"}', mockContext);
    
    assert(result.success);
    assert(result.data.status === 'requested');
  });

  test('should handle invalid action', async () => {
    const { command, mockContext } = createTestContext();
    const result = await command.execute('{"action": "invalid"}', mockContext);
    
    assert(!result.success);
    assert(result.error.includes('Invalid action'));
  });

  test('should handle malformed parameters', async () => {
    const { command, mockContext } = createTestContext();
    const result = await command.execute('invalid json', mockContext);
    
    assert(!result.success);
    assert(result.error.includes('failed'));
  });

  test('should provide usage examples', () => {
    const { command } = createTestContext();
    const definition = command.getDefinition();
    assert(Array.isArray(definition.examples));
    assert(definition.examples.length >= 4);
    
    const examples = definition.examples.map(ex => ex.usage);
    assert(examples.some(ex => ex.includes('activate')));
    assert(examples.some(ex => ex.includes('deactivate')));
    assert(examples.some(ex => ex.includes('position')));
    assert(examples.some(ex => ex.includes('status')));
  });
});