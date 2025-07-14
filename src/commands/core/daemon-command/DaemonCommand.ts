// ISSUES: 1 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
// 🎯 ARCHITECTURAL CHANGE: Converting to typed parameter execution pattern
/**
 * DaemonCommand - Base class for commands that delegate to daemons
 * 
 * Commands that need to communicate with daemons should extend this class
 * and specify which daemon handles the operation.
 */

import { BaseCommand, CommandResult, CommandContext } from '../base-command/BaseCommand';

export interface DaemonRequest {
  targetDaemon: string;
  messageType: string;
  data: any;
}

export abstract class DaemonCommand extends BaseCommand {
  /**
   * Subclasses must override to specify which daemon handles this command
   */
  protected static getTargetDaemon(): string {
    throw new Error('getTargetDaemon() must be implemented by subclass');
  }
  
  /**
   * Subclasses must override to specify the message type for the daemon
   */
  protected static getMessageType(): string {
    throw new Error('getMessageType() must be implemented by subclass');
  }
  
  /**
   * Subclasses must override to prepare the data to send to the daemon
   */
  protected static prepareDaemonData(_params: any, _context?: CommandContext): any {
    throw new Error('prepareDaemonData() must be implemented by subclass');
  }
  
  /**
   * Standard execute that delegates to daemon
   */
  static async execute(params: any, context?: CommandContext): Promise<CommandResult> {
    try {
      // Parameters are automatically parsed by UniversalCommandRegistry
      // Create daemon request
      const daemonRequest: DaemonRequest = {
        targetDaemon: this.getTargetDaemon(),
        messageType: this.getMessageType(),
        data: this.prepareDaemonData(params, context)
      };
      
      // Return a result that tells CommandProcessor to route through daemon
      return {
        success: true,
        data: {
          _routeToDaemon: daemonRequest
        },
        message: `Routing to ${daemonRequest.targetDaemon}`,
        timestamp: new Date().toISOString()
      };
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Command failed: ${errorMessage}`);
    }
  }
}