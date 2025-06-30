/**
 * Console Command - Bridge browser console logs back to development portal
 * 
 * CRITICAL FOR JTAG: This enables autonomous development by forwarding
 * browser console logs, errors, and health reports back to the AI portal
 */

import { DirectCommand } from '../direct-command/DirectCommand.js';
import { CommandResult, CommandContext, CommandDefinition } from '../base-command/BaseCommand.js';

export class ConsoleCommand extends DirectCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'console',
      description: 'Forward browser console logs to development portal for JTAG autonomous development',
      category: 'core',
      parameters: {
        action: {
          type: 'string',
          description: 'log, error, warn, info, health_report',
          required: true
        },
        message: {
          type: 'string', 
          description: 'Console message content',
          required: true
        },
        source: {
          type: 'string',
          description: 'Source component (widget, api, etc)',
          required: false
        },
        data: {
          type: 'object',
          description: 'Additional data (health metrics, error details, etc)',
          required: false
        }
      },
      examples: [
        {
          description: 'Forward health report',
          command: 'console --action=health_report --message="System healthy" --data=healthReport'
        },
        {
          description: 'Forward error',
          command: 'console --action=error --message="Widget failed to load" --source=chat-widget'
        }
      ]
    };
  }

  protected static async executeOperation(params: any = {}, _context?: CommandContext): Promise<CommandResult> {
    try {
      const { action, message, source, data } = params;
      
      if (!action || !message) {
        return this.createErrorResult('Console forwarding requires action and message parameters');
      }

      // Format console message for portal
      const timestamp = new Date().toISOString();
      const consoleEntry = {
        timestamp,
        action,
        message,
        source: source || 'browser',
        data: data || {},
        environment: 'browser'
      };

      // TODO: Forward to actual development portal
      // For now, log to server console for JTAG development
      const iconMap: Record<string, string> = {
        'log': '📝',
        'error': '❌', 
        'warn': '⚠️',
        'info': 'ℹ️',
        'health_report': '🏥'
      };
      const icon = iconMap[action as string] || '📝';

      console.log(`${icon} PORTAL BRIDGE [${source}]: ${message}`);
      if (data && Object.keys(data).length > 0) {
        console.log(`   Data:`, data);
      }

      return this.createSuccessResult(
        'Console message forwarded successfully',
        {
          forwarded: true,
          timestamp,
          consoleEntry
        }
      );
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Console forwarding failed: ${errorMessage}`);
    }
  }
}