const BaseCommand = require('../../BaseCommand.cjs');
const { exec } = require('child_process');
const { promisify } = require('util');

const execAsync = promisify(exec);

/**
 * Input Command - Comprehensive input event handling
 * Handles mouse, touch, keyboard, and gesture events in one unified command
 */
class InputCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'INPUT',
      category: 'input control',
      icon: '🎮',
      description: 'Unified input event handling: mouse, touch, keyboard, gestures',
      parameters: {
        event: {
          type: 'string',
          required: true,
          description: 'Event type: click, touch, mousedown, mouseup, mousemove, drag, scroll, key'
        },
        x: {
          type: 'number',
          required: false,
          description: 'X coordinate (for positional events)'
        },
        y: {
          type: 'number',
          required: false,
          description: 'Y coordinate (for positional events)'
        },
        button: {
          type: 'string',
          required: false,
          default: 'left',
          description: 'Mouse button: left, right, middle'
        },
        key: {
          type: 'string',
          required: false,
          description: 'Key name for keyboard events'
        },
        text: {
          type: 'string',
          required: false,
          description: 'Text to type'
        },
        duration: {
          type: 'number',
          required: false,
          description: 'Event duration in milliseconds'
        },
        animation: {
          type: 'string',
          required: false,
          default: 'natural',
          description: 'Animation style: natural, instant, smooth'
        }
      },
      examples: [
        '{"event": "click", "x": 100, "y": 200}',
        '{"event": "key", "text": "Hello World"}',
        '{"event": "scroll", "direction": "down"}',
        '{"event": "type", "text": "Hello world!", "animation": "natural"}'
      ],
      usage: 'Handles all types of input events in a unified interface'
    };
  }


  static async execute(params, context) {
    try {
      const { event, x, y, button = 'left', key, text, duration, animation = 'natural' } = this.parseParams(params);

      switch (event.toLowerCase()) {
        case 'click':
          return await this.handleClick(x, y, button, animation, context);
        case 'touch':
          return await this.handleTouch(x, y, duration, context);
        case 'mousedown':
          return await this.handleMouseDown(x, y, button, context);
        case 'mouseup':
          return await this.handleMouseUp(x, y, button, context);
        case 'mousemove':
          return await this.handleMouseMove(x, y, animation, context);
        case 'drag':
          return await this.handleDrag(params, context);
        case 'scroll':
          return await this.handleScroll(x, y, params, context);
        case 'key':
          return await this.handleKey(key, context);
        case 'type':
          return await this.handleType(text, animation, context);
        default:
          return this.createErrorResult(`Unknown event type: ${event}`);
      }

    } catch (error) {
      console.error('❌ Input command failed:', error);
      return this.createErrorResult(`Input command failed: ${error.message}`);
    }
  }

  static async handleClick(x, y, button, animation, context) {
    const position = await this.resolvePosition(x, y);
    console.log(`🖱️ Click ${button} at (${position.x}, ${position.y})`);

    let command;
    switch (button) {
      case 'left':
        command = `cliclick c:${position.x},${position.y}`;
        break;
      case 'right':
        command = `cliclick rc:${position.x},${position.y}`;
        break;
      case 'middle':
        command = `cliclick mc:${position.x},${position.y}`;
        break;
      default:
        return this.createErrorResult(`Invalid button: ${button}`);
    }

    if (animation === 'natural') {
      command = `cliclick m:${position.x},${position.y} && sleep 0.1 && ${command}`;
    }

    await execAsync(command);
    this.sendFeedback('click', { position, button }, context);

    return this.createSuccessResult({
      event: 'click',
      position,
      button,
      timestamp: new Date().toISOString()
    });
  }

  static async handleTouch(x, y, duration, context) {
    const position = await this.resolvePosition(x, y);
    console.log(`👆 Touch at (${position.x}, ${position.y}) for ${duration || 'instant'}`);

    // Touch is similar to click but with optional duration
    let command = `cliclick c:${position.x},${position.y}`;
    
    if (duration && duration > 0) {
      // Simulate long press with mouse down/up
      command = `cliclick dd:${position.x},${position.y} && sleep ${duration / 1000} && cliclick du:${position.x},${position.y}`;
    }

    await execAsync(command);
    this.sendFeedback('touch', { position, duration }, context);

    return this.createSuccessResult({
      event: 'touch',
      position,
      duration,
      timestamp: new Date().toISOString()
    });
  }

  static async handleMouseDown(x, y, button, context) {
    const position = await this.resolvePosition(x, y);
    console.log(`⬇️ Mouse down ${button} at (${position.x}, ${position.y})`);

    let command;
    switch (button) {
      case 'left':
        command = `cliclick dd:${position.x},${position.y}`;
        break;
      case 'right':
        command = `cliclick rdd:${position.x},${position.y}`;
        break;
      case 'middle':
        command = `cliclick mdd:${position.x},${position.y}`;
        break;
      default:
        return this.createErrorResult(`Invalid button: ${button}`);
    }

    await execAsync(command);
    this.sendFeedback('mousedown', { position, button }, context);

    return this.createSuccessResult({
      event: 'mousedown',
      position,
      button,
      timestamp: new Date().toISOString()
    });
  }

  static async handleMouseUp(x, y, button, context) {
    const position = await this.resolvePosition(x, y);
    console.log(`⬆️ Mouse up ${button} at (${position.x}, ${position.y})`);

    let command;
    switch (button) {
      case 'left':
        command = `cliclick du:${position.x},${position.y}`;
        break;
      case 'right':
        command = `cliclick rdu:${position.x},${position.y}`;
        break;
      case 'middle':
        command = `cliclick mdu:${position.x},${position.y}`;
        break;
      default:
        return this.createErrorResult(`Invalid button: ${button}`);
    }

    await execAsync(command);
    this.sendFeedback('mouseup', { position, button }, context);

    return this.createSuccessResult({
      event: 'mouseup',
      position,
      button,
      timestamp: new Date().toISOString()
    });
  }

  static async handleMouseMove(x, y, animation, context) {
    if (x === undefined || y === undefined) {
      return this.createErrorResult('X and Y coordinates required for mouse move');
    }

    console.log(`🎯 Mouse move to (${x}, ${y})`);

    let command = `cliclick m:${x},${y}`;
    
    if (animation === 'natural') {
      // Add natural movement curve
      command = await this.createNaturalMovement({ x: 0, y: 0 }, { x, y });
    }

    await execAsync(command);
    this.sendFeedback('mousemove', { position: { x, y } }, context);

    return this.createSuccessResult({
      event: 'mousemove',
      position: { x, y },
      animation,
      timestamp: new Date().toISOString()
    });
  }

  static async handleDrag(params, context) {
    const { x, y, x2, y2, button = 'left' } = params;
    
    if (!x2 || !y2) {
      return this.createErrorResult('Drag requires start (x,y) and end (x2,y2) coordinates');
    }

    console.log(`↔️ Drag from (${x}, ${y}) to (${x2}, ${y2})`);

    const startPos = await this.resolvePosition(x, y);
    const command = `cliclick dd:${startPos.x},${startPos.y} && cliclick m:${x2},${y2} && cliclick du:${x2},${y2}`;

    await execAsync(command);
    this.sendFeedback('drag', { from: startPos, to: { x: x2, y: y2 } }, context);

    return this.createSuccessResult({
      event: 'drag',
      from: startPos,
      to: { x: x2, y: y2 },
      timestamp: new Date().toISOString()
    });
  }

  static async handleScroll(x, y, params, context) {
    const { direction = 'down', amount = 5 } = params;
    const position = await this.resolvePosition(x, y);
    
    console.log(`📜 Scroll ${direction} at (${position.x}, ${position.y})`);

    // Move to position first, then scroll
    let command = `cliclick m:${position.x},${position.y} && `;
    
    for (let i = 0; i < amount; i++) {
      if (direction === 'up') {
        command += 'cliclick w:+1 && ';
      } else {
        command += 'cliclick w:-1 && ';
      }
    }
    
    command = command.slice(0, -4); // Remove trailing ' && '

    await execAsync(command);
    this.sendFeedback('scroll', { position, direction, amount }, context);

    return this.createSuccessResult({
      event: 'scroll',
      position,
      direction,
      amount,
      timestamp: new Date().toISOString()
    });
  }

  static async handleKey(key, context) {
    if (!key) {
      return this.createErrorResult('Key parameter required for key events');
    }

    console.log(`⌨️ Key press: ${key}`);

    const command = `cliclick key:${key}`;
    await execAsync(command);
    this.sendFeedback('key', { key }, context);

    return this.createSuccessResult({
      event: 'key',
      key,
      timestamp: new Date().toISOString()
    });
  }

  static async handleType(text, animation, context) {
    if (!text) {
      return this.createErrorResult('Text parameter required for type events');
    }

    console.log(`⌨️ Type: ${text.substring(0, 50)}${text.length > 50 ? '...' : ''}`);

    const escapedText = text.replace(/'/g, "'\"'\"'");
    let command;

    if (animation === 'instant') {
      command = `cliclick t:'${escapedText}'`;
    } else {
      // Natural typing with character-by-character delays
      command = await this.createNaturalTyping(escapedText);
    }

    await execAsync(command);
    this.sendFeedback('type', { text: text.substring(0, 100) }, context);

    return this.createSuccessResult({
      event: 'type',
      text,
      length: text.length,
      timestamp: new Date().toISOString()
    });
  }

  static async resolvePosition(x, y) {
    if (x !== undefined && y !== undefined) {
      return { x, y };
    }

    // Get current cursor position if not specified
    try {
      const { stdout } = await execAsync('cliclick p');
      const coords = stdout.trim().split(',');
      return {
        x: x || parseInt(coords[0]),
        y: y || parseInt(coords[1])
      };
    } catch (error) {
      throw new Error('Could not determine cursor position');
    }
  }

  static async createNaturalMovement(from, to) {
    // Simple natural movement for now
    return `cliclick m:${to.x},${to.y}`;
  }

  static async createNaturalTyping(text) {
    const commands = [];
    for (let i = 0; i < text.length; i++) {
      const char = text[i];
      commands.push(`cliclick t:'${char}'`);
      if (i < text.length - 1) {
        const delay = 0.08 + (Math.random() * 0.04); // 80-120ms
        commands.push(`sleep ${delay}`);
      }
    }
    return commands.join(' && ');
  }

  static sendFeedback(eventType, data, context) {
    if (context && context.webSocketServer) {
      context.webSocketServer.broadcast({
        type: 'input_feedback',
        event: eventType,
        data,
        timestamp: new Date().toISOString()
      });
    }
  }

}

module.exports = InputCommand;