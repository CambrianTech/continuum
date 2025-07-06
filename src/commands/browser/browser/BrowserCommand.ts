/**
 * Browser Command - Launches browser via BrowserManagerDaemon
 */

import { DirectCommand } from '../../core/direct-command/DirectCommand';
import { CommandDefinition, CommandResult, CommandContext } from '../../core/base-command/BaseCommand';
import { DAEMON_EVENT_BUS } from '../../../daemons/base/DaemonEventBus';

interface BrowserParams {
  url?: string;
  sessionId?: string;
}

export class BrowserCommand extends DirectCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'browser',
      category: 'Browser',
      icon: '🌐',
      description: 'Launch browser and navigate to URL',
      parameters: { 
        url: { type: 'string' as const, description: 'URL to navigate to (default: http://localhost:9000)' },
        sessionId: { type: 'string' as const, description: 'Session ID to associate with browser' }
      },
      examples: [
        { description: 'Launch browser to localhost', command: '{}' },
        { description: 'Navigate to URL', command: '{"url": "https://example.com"}' }
      ],
      usage: 'Launch browser and navigate to Continuum interface or specified URL'
    };
  }

  protected static async executeOperation(params: BrowserParams, context?: CommandContext): Promise<CommandResult> {
    try {
      const url = params.url || 'http://localhost:9000';
      const sessionId = params.sessionId || context?.sessionId;

      // Send browser creation request to BrowserManagerDaemon
      const browserRequest = {
        type: 'browser_request',
        action: 'create',
        data: {
          url: url,
          sessionId: sessionId,
          browserType: 'chrome',
          config: {
            headless: false,
            newWindow: true
          }
        }
      };

      // Emit event to BrowserManagerDaemon
      DAEMON_EVENT_BUS.emit('browser_request', browserRequest);

      return this.createSuccessResult({
        message: `Browser launched with URL: ${url}`,
        url: url,
        sessionId: sessionId
      });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Failed to launch browser: ${errorMessage}`);
    }
  }
}

export default BrowserCommand;