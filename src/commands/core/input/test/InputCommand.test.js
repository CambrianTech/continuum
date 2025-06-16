/**
 * Input Command Tests
 * Tests comprehensive input event handling: mouse, touch, keyboard, gestures
 */

import { test, describe } from 'node:test';
import assert from 'node:assert';
import { createRequire } from 'module';

const require = createRequire(import.meta.url);
const InputCommand = require('../InputCommand.cjs');

describe('Input Command', () => {
  let command;
  let mockContext;

  beforeEach(() => {
    command = new InputCommand();
    
    // Mock WebSocket context and exec calls
    mockContext = {
      webSocketServer: {
        broadcast: (message) => {
          mockContext.lastBroadcast = message;
        }
      },
      lastBroadcast: null
    };

    // Mock exec to avoid actual system calls during tests
    command.execAsync = async (cmd) => {
      mockContext.lastExecCommand = cmd;
      if (cmd.includes('cliclick p')) {
        return { stdout: '400,300', stderr: '' };
      }
      return { stdout: 'success', stderr: '' };
    };
  });

  test('should be properly configured', () => {
    assert.strictEqual(command.name, 'input');
    assert.strictEqual(command.description, 'Unified input event handling: mouse, touch, keyboard, gestures');
    assert.strictEqual(command.icon, '🎮');
    assert.strictEqual(command.category, 'input control');
  });

  test('should have comprehensive parameters', () => {
    const definition = command.getDefinition();
    assert(definition.parameters.event);
    assert(definition.parameters.event.required);
    assert(definition.parameters.x);
    assert(definition.parameters.y);
    assert(definition.parameters.button);
    assert(definition.parameters.key);
    assert(definition.parameters.text);
    assert(definition.parameters.duration);
    assert(definition.parameters.animation);
  });

  describe('click event', () => {
    test('should handle left click at coordinates', async () => {
      const result = await command.execute('{"event": "click", "x": 500, "y": 400, "button": "left"}', mockContext);
      
      assert(result.success);
      assert.strictEqual(result.data.event, 'click');
      assert.deepStrictEqual(result.data.position, { x: 500, y: 400 });
      assert.strictEqual(result.data.button, 'left');
      
      // Check WebSocket feedback
      assert(mockContext.lastBroadcast);
      assert.strictEqual(mockContext.lastBroadcast.type, 'input_feedback');
      assert.strictEqual(mockContext.lastBroadcast.event, 'click');
    });

    test('should handle right click', async () => {
      const result = await command.execute('{"event": "click", "x": 300, "y": 200, "button": "right"}', mockContext);
      
      assert(result.success);
      assert.strictEqual(result.data.button, 'right');
      assert(mockContext.lastExecCommand.includes('rc:'));
    });

    test('should handle middle click', async () => {
      const result = await command.execute('{"event": "click", "x": 300, "y": 200, "button": "middle"}', mockContext);
      
      assert(result.success);
      assert.strictEqual(result.data.button, 'middle');
      assert(mockContext.lastExecCommand.includes('mc:'));
    });

    test('should use current position when coordinates not provided', async () => {
      const result = await command.execute('{"event": "click", "button": "left"}', mockContext);
      
      assert(result.success);
      // Should use mocked current position (400,300)
      assert.deepStrictEqual(result.data.position, { x: 400, y: 300 });
    });

    test('should fail with invalid button', async () => {
      const result = await command.execute('{"event": "click", "x": 300, "y": 200, "button": "invalid"}', mockContext);
      
      assert(!result.success);
      assert(result.error.includes('Invalid button'));
    });
  });

  describe('touch event', () => {
    test('should handle basic touch', async () => {
      const result = await command.execute('{"event": "touch", "x": 600, "y": 500}', mockContext);
      
      assert(result.success);
      assert.strictEqual(result.data.event, 'touch');
      assert.deepStrictEqual(result.data.position, { x: 600, y: 500 });
    });

    test('should handle long press with duration', async () => {
      const result = await command.execute('{"event": "touch", "x": 600, "y": 500, "duration": 1000}', mockContext);
      
      assert(result.success);
      assert.strictEqual(result.data.duration, 1000);
      assert(mockContext.lastExecCommand.includes('dd:') && mockContext.lastExecCommand.includes('du:'));
    });
  });

  describe('mouse events', () => {
    test('should handle mousedown', async () => {
      const result = await command.execute('{"event": "mousedown", "x": 100, "y": 200, "button": "left"}', mockContext);
      
      assert(result.success);
      assert.strictEqual(result.data.event, 'mousedown');
      assert.strictEqual(result.data.button, 'left');
      assert(mockContext.lastExecCommand.includes('dd:'));
    });

    test('should handle mouseup', async () => {
      const result = await command.execute('{"event": "mouseup", "x": 100, "y": 200, "button": "right"}', mockContext);
      
      assert(result.success);
      assert.strictEqual(result.data.event, 'mouseup');
      assert.strictEqual(result.data.button, 'right');
      assert(mockContext.lastExecCommand.includes('rdu:'));
    });

    test('should handle mousemove', async () => {
      const result = await command.execute('{"event": "mousemove", "x": 800, "y": 600}', mockContext);
      
      assert(result.success);
      assert.strictEqual(result.data.event, 'mousemove');
      assert.deepStrictEqual(result.data.position, { x: 800, y: 600 });
      assert(mockContext.lastExecCommand.includes('m:'));
    });

    test('should fail mousemove without coordinates', async () => {
      const result = await command.execute('{"event": "mousemove"}', mockContext);
      
      assert(!result.success);
      assert(result.error.includes('coordinates required'));
    });
  });

  describe('drag event', () => {
    test('should handle drag operation', async () => {
      const result = await command.execute('{"event": "drag", "x": 100, "y": 100, "x2": 300, "y2": 400}', mockContext);
      
      assert(result.success);
      assert.strictEqual(result.data.event, 'drag');
      assert.deepStrictEqual(result.data.from, { x: 100, y: 100 });
      assert.deepStrictEqual(result.data.to, { x: 300, y: 400 });
      
      // Should contain drag sequence
      assert(mockContext.lastExecCommand.includes('dd:') && 
             mockContext.lastExecCommand.includes('m:') && 
             mockContext.lastExecCommand.includes('du:'));
    });

    test('should fail drag without end coordinates', async () => {
      const result = await command.execute('{"event": "drag", "x": 100, "y": 100}', mockContext);
      
      assert(!result.success);
      assert(result.error.includes('end (x2,y2) coordinates'));
    });
  });

  describe('scroll event', () => {
    test('should handle scroll down', async () => {
      const result = await command.execute('{"event": "scroll", "x": 400, "y": 300, "direction": "down", "amount": 3}', mockContext);
      
      assert(result.success);
      assert.strictEqual(result.data.event, 'scroll');
      assert.strictEqual(result.data.direction, 'down');
      assert.strictEqual(result.data.amount, 3);
      assert(mockContext.lastExecCommand.includes('w:-1'));
    });

    test('should handle scroll up', async () => {
      const result = await command.execute('{"event": "scroll", "x": 400, "y": 300, "direction": "up", "amount": 2}', mockContext);
      
      assert(result.success);
      assert.strictEqual(result.data.direction, 'up');
      assert(mockContext.lastExecCommand.includes('w:+1'));
    });
  });

  describe('keyboard events', () => {
    test('should handle key press', async () => {
      const result = await command.execute('{"event": "key", "key": "return"}', mockContext);
      
      assert(result.success);
      assert.strictEqual(result.data.event, 'key');
      assert.strictEqual(result.data.key, 'return');
      assert(mockContext.lastExecCommand.includes('key:return'));
    });

    test('should fail key press without key parameter', async () => {
      const result = await command.execute('{"event": "key"}', mockContext);
      
      assert(!result.success);
      assert(result.error.includes('Key parameter required'));
    });

    test('should handle type event', async () => {
      const result = await command.execute('{"event": "type", "text": "Hello World", "animation": "instant"}', mockContext);
      
      assert(result.success);
      assert.strictEqual(result.data.event, 'type');
      assert.strictEqual(result.data.text, 'Hello World');
      assert.strictEqual(result.data.length, 11);
    });

    test('should fail type without text', async () => {
      const result = await command.execute('{"event": "type"}', mockContext);
      
      assert(!result.success);
      assert(result.error.includes('Text parameter required'));
    });
  });

  describe('animation support', () => {
    test('should support natural animation for click', async () => {
      const result = await command.execute('{"event": "click", "x": 500, "y": 400, "animation": "natural"}', mockContext);
      
      assert(result.success);
      // Natural animation should include movement before click
      assert(mockContext.lastExecCommand.includes('m:') && mockContext.lastExecCommand.includes('sleep'));
    });

    test('should support instant execution', async () => {
      const result = await command.execute('{"event": "click", "x": 500, "y": 400, "animation": "instant"}', mockContext);
      
      assert(result.success);
      // Instant should not include sleep delays
      assert(!mockContext.lastExecCommand.includes('sleep'));
    });
  });

  describe('error handling', () => {
    test('should handle unknown event type', async () => {
      const result = await command.execute('{"event": "unknown"}', mockContext);
      
      assert(!result.success);
      assert(result.error.includes('Unknown event type'));
    });

    test('should handle malformed JSON', async () => {
      const result = await command.execute('invalid json', mockContext);
      
      assert(!result.success);
      assert(result.error.includes('failed'));
    });

    test('should handle system command failures', async () => {
      // Mock exec failure
      command.execAsync = async () => {
        throw new Error('Command failed');
      };

      const result = await command.execute('{"event": "click", "x": 100, "y": 200}', mockContext);
      
      assert(!result.success);
      assert(result.error.includes('failed'));
    });
  });

  describe('examples and documentation', () => {
    test('should provide comprehensive usage examples', () => {
      const definition = command.getDefinition();
      assert(Array.isArray(definition.examples));
      assert(definition.examples.length >= 7);
      
      const examples = definition.examples.map(ex => ex.usage);
      assert(examples.some(ex => ex.includes('click')));
      assert(examples.some(ex => ex.includes('touch')));
      assert(examples.some(ex => ex.includes('drag')));
      assert(examples.some(ex => ex.includes('scroll')));
      assert(examples.some(ex => ex.includes('key')));
      assert(examples.some(ex => ex.includes('type')));
    });

    test('should document all event types', () => {
      const definition = command.getDefinition();
      const eventParam = definition.parameters.event;
      assert(eventParam.description.includes('click'));
      assert(eventParam.description.includes('touch'));
      assert(eventParam.description.includes('key'));
    });
  });
});