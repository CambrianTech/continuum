const BaseCommand = require('../../BaseCommand.cjs');

/**
 * Cursor Command - Controls the continuon (green AI cursor) activation and positioning
 * Manages the visual representation of AI control over the interface
 */
class CursorCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'CURSOR',
      category: 'continuon control',
      icon: '🟢',
      description: 'Control continuon (AI cursor) activation, deactivation, and positioning',
      params: '{"action": "activate|deactivate|position|status", "x": number, "y": number}',
      examples: [
        '{"action": "activate"}',
        '{"action": "position", "x": 100, "y": 200}',
        '{"action": "status"}'
      ],
      usage: 'Controls the visual AI cursor representation'
    };
  }

  constructor() {
    super();
    this.name = 'cursor';
    this.description = 'Control continuon (AI cursor) activation, deactivation, and positioning';
    this.icon = '🟢';
    this.category = 'continuon control';
    this.parameters = {
      action: {
        type: 'string',
        required: true,
        description: 'Action: activate, deactivate, position, status'
      },
      x: {
        type: 'number',
        required: false,
        description: 'X coordinate for positioning (optional)'
      },
      y: {
        type: 'number',
        required: false,
        description: 'Y coordinate for positioning (optional)'
      },
      state: {
        type: 'string',
        required: false,
        default: 'normal',
        description: 'System state: normal, error, warning, processing, success'
      },
      speed: {
        type: 'string',
        required: false,
        default: 'natural',
        description: 'Movement speed: slow, natural, fast, instant'
      },
      curve: {
        type: 'string',
        required: false,
        default: 'bezier',
        description: 'Movement curve: linear, bezier, arc, bounce'
      }
    };
  }

  async execute(params, context) {
    try {
      const { action, x, y, state = 'normal', speed = 'natural', curve = 'bezier' } = this.parseParams(params);

      switch (action.toLowerCase()) {
        case 'activate':
          return await this.activateCursor(context);
        case 'deactivate':
          return await this.deactivateCursor(context);
        case 'position':
          return await this.positionCursor(x, y, { state, speed, curve }, context);
        case 'status':
          return await this.getCursorStatus(context);
        default:
          return this.createErrorResult(`Invalid action: ${action}. Use: activate, deactivate, position, status`);
      }

    } catch (error) {
      console.error('❌ Cursor command failed:', error);
      return this.createErrorResult(`Cursor command failed: ${error.message}`);
    }
  }

  async activateCursor(context) {
    console.log('🟢 Activating continuon cursor...');

    if (context && context.webSocketServer) {
      // Send activation command to browser
      context.webSocketServer.broadcast({
        type: 'cursor_control',
        action: 'activate',
        timestamp: new Date().toISOString()
      });

      console.log('✅ Continuon cursor activated - HAL 9000 is now the mouse pointer');
      
      return this.createSuccessResult({
        activated: true,
        message: 'Continuon cursor activated - AI visual control enabled',
        timestamp: new Date().toISOString()
      });
    } else {
      return this.createErrorResult('No browser connection available for cursor control');
    }
  }

  async deactivateCursor(context) {
    console.log('🟢 Deactivating continuon cursor...');

    if (context && context.webSocketServer) {
      // Send deactivation command to browser
      context.webSocketServer.broadcast({
        type: 'cursor_control',
        action: 'deactivate',
        timestamp: new Date().toISOString()
      });

      console.log('✅ Continuon cursor deactivated - returning to home position');
      
      return this.createSuccessResult({
        deactivated: true,
        message: 'Continuon cursor deactivated - returned to base position',
        timestamp: new Date().toISOString()
      });
    } else {
      return this.createErrorResult('No browser connection available for cursor control');
    }
  }

  async positionCursor(x, y, options, context) {
    if (x === undefined || y === undefined) {
      return this.createErrorResult('X and Y coordinates required for positioning');
    }

    if (typeof x !== 'number' || typeof y !== 'number') {
      return this.createErrorResult('Invalid coordinates provided');
    }

    const { color, speed, curve } = options;
    console.log(`🟢 Positioning continuon cursor to (${x}, ${y}) [${color}, ${speed}, ${curve}]`);

    if (context && context.webSocketServer) {
      // Send positioning command to browser with movement parameters
      context.webSocketServer.broadcast({
        type: 'cursor_control',
        action: 'position',
        position: { x, y },
        movement: {
          color,
          speed,
          curve,
          bezier: this.getBezierCurve(curve),
          duration: this.getMovementDuration(speed)
        },
        timestamp: new Date().toISOString()
      });

      console.log(`✅ Continuon cursor positioned at (${x}, ${y}) with ${speed} ${curve} movement`);
      
      return this.createSuccessResult({
        positioned: true,
        position: { x, y },
        movement: { color, speed, curve },
        message: `Continuon cursor moved to (${x}, ${y}) with graceful ${speed} motion`,
        timestamp: new Date().toISOString()
      });
    } else {
      return this.createErrorResult('No browser connection available for cursor control');
    }
  }

  /**
   * Get Bezier curve parameters for different movement types
   */
  getBezierCurve(curveType) {
    const curves = {
      linear: [0, 0, 1, 1],
      bezier: [0.25, 0.46, 0.45, 0.94],  // Natural ease-in-out
      arc: [0.68, -0.55, 0.265, 1.55],   // Bounce effect
      bounce: [0.175, 0.885, 0.32, 1.275], // Gentle bounce
      smooth: [0.4, 0, 0.2, 1],          // Material design
      swift: [0.4, 0, 0.6, 1]            // Fast but smooth
    };
    return curves[curveType] || curves.bezier;
  }

  /**
   * Get movement duration based on speed setting
   */
  getMovementDuration(speed) {
    const durations = {
      slow: 2000,     // 2 seconds
      natural: 1200,  // 1.2 seconds  
      fast: 600,      // 0.6 seconds
      instant: 0      // No animation
    };
    return durations[speed] || durations.natural;
  }

  async getCursorStatus(context) {
    if (context && context.webSocketServer) {
      // Request status from browser
      context.webSocketServer.broadcast({
        type: 'cursor_control',
        action: 'status',
        timestamp: new Date().toISOString()
      });

      // For now, return basic status (in real implementation, we'd wait for browser response)
      return this.createSuccessResult({
        status: 'requested',
        message: 'Cursor status requested from browser',
        timestamp: new Date().toISOString()
      });
    } else {
      return this.createErrorResult('No browser connection available for cursor status');
    }
  }

  getDefinition() {
    return {
      name: this.name,
      description: this.description,
      icon: this.icon,
      category: this.category,
      parameters: this.parameters,
      examples: [
        {
          description: 'Activate the continuon cursor',
          usage: 'cursor activate'
        },
        {
          description: 'Deactivate the continuon cursor',
          usage: 'cursor deactivate'
        },
        {
          description: 'Position continuon cursor at specific coordinates',
          usage: 'cursor position 400 300'
        },
        {
          description: 'Position cursor with red color for error indication',
          usage: 'cursor position 400 300 --color #ff0000 --speed slow'
        },
        {
          description: 'Move cursor with bouncy animation',
          usage: 'cursor position 600 400 --curve bounce --speed natural'
        },
        {
          description: 'Get current cursor status',
          usage: 'cursor status'
        }
      ]
    };
  }
}

module.exports = CursorCommand;