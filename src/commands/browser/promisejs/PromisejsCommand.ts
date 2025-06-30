/**
 * PromiseJS Command - Stub implementation
 * TODO: Implement promise-based JavaScript execution in browser
 */

import { RemoteCommand, RemoteExecutionRequest, RemoteExecutionResponse } from '../../core/remote-command/RemoteCommand.js';
import { CommandDefinition } from '../../core/base-command/BaseCommand.js';

export class PromisejsCommand extends RemoteCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'promisejs',
      category: 'Browser',
      icon: '⚡',
      description: 'Promise-based JavaScript execution in browser (stub implementation)',
      parameters: { script: 'string' },
      examples: [
        { description: 'Execute promise script', command: '{"script": "Promise.resolve(document.title)"}' }
      ],
      usage: 'Execute promise-based JavaScript code in browser environment'
    };
  }

  protected static async executeOnClient(_request: RemoteExecutionRequest): Promise<RemoteExecutionResponse> {
    return {
      success: false,
      error: 'PromiseJS command not yet implemented - stub only',
      clientMetadata: {
        userAgent: 'stub',
        timestamp: Date.now(),
        executionTime: 0
      }
    };
  }
}

export default PromisejsCommand;