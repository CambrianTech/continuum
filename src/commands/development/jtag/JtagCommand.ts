/**
 * JTAG Command - Ultra-Simple Browser Forwarder
 * =============================================
 * Just forwards CLI arguments to browser JTAG system
 */

import { BaseCommand, CommandResult } from '../../core/base-command/BaseCommand';
import { COMMAND_CATEGORIES } from '../../../types/shared/CommandTypes';

export class JtagCommand extends BaseCommand {
  static getDefinition() {
    return {
      name: 'jtag',
      category: COMMAND_CATEGORIES.DEVELOPMENT,
      description: 'JTAG AI Autonomous Debugging - Browser Widget Analysis',
      icon: '🛸',
      parameters: {
        method: {
          type: 'string' as const,
          description: 'JTAG method: widgets, health, shadowDOM, network, performance, execute',
          required: true
        }
      },
      examples: [
        {
          description: 'Analyze widgets',
          command: 'jtag --method=widgets'
        },
        {
          description: 'Check system health',
          command: 'jtag --method=health'
        },
        {
          description: 'Analyze shadow DOM structure',
          command: 'jtag --method=shadowDOM'
        }
      ]
    };
  }

  static async execute(params: any): Promise<CommandResult> {
    try {
      console.log(`🔍 JTAG received:`, JSON.stringify(params));
      
      // Expect simple method parameter - no complex CLI parsing needed
      const method = params.method || 'health'; // default to health
      const options = params.options || {};
      
      console.log(`🔍 JTAG executing: ${method}`);
      
      // Forward to browser JTAG - simple delegation
      const probeCode = `
        if (!window.jtag) {
          return { error: 'JTAG browser API not available. Run: npm start first.' };
        }
        
        try {
          const method = '${method}';
          const options = ${JSON.stringify(options)};
          
          if (typeof window.jtag[method] === 'function') {
            return window.jtag[method](options);
          } else {
            return { error: 'JTAG method not available: ' + method + '. Available: ' + Object.keys(window.jtag).join(', ') };
          }
        } catch (error) {
          return { error: 'JTAG execution failed: ' + error.message };
        }
      `;
      
      // Execute JavaScript in browser
      const { JSExecuteCommand } = await import('../../browser/js-execute/JSExecuteCommand');
      const executeResult = await JSExecuteCommand.execute({ code: probeCode });
      
      return executeResult;
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        data: { rawParams: params }
      };
    }
  }
}