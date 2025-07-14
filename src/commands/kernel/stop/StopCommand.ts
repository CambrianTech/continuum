/**
 * Stop Command - Stop the Continuum daemon system
 */

import { DirectCommand } from '../../core/direct-command/DirectCommand';
import { CommandDefinition, ContinuumContext, CommandResult } from '../../core/base-command/BaseCommand';

export class StopCommand extends DirectCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'stop',
      category: 'kernel',
      icon: '🛑',
      description: 'Stop the Continuum daemon system',
      parameters: {},
      examples: [
        { 
          description: 'Stop daemon system', 
          command: `{}` 
        }
      ],
      usage: 'Stop the Continuum daemon operating system'
    };
  }

  protected static async executeOperation(_params: any = {}, _context?: ContinuumContext): Promise<CommandResult> {
    try {
      // Check if daemons are running
      try {
        const response = await fetch('http://localhost:9000/api/status');
        if (!response.ok) {
          return this.createSuccessResult(
            `Continuum daemons already stopped`,
            {
              status: 'already_stopped',
              interface: 'http://localhost:9000'
            }
          );
        }
      } catch {
        return this.createSuccessResult(
          `Continuum daemons already stopped`,
          {
            status: 'already_stopped',
            interface: 'http://localhost:9000'
          }
        );
      }

      // Stop the daemon system by killing processes
      const { exec } = await import('child_process');
      const { promisify } = await import('util');
      const execAsync = promisify(exec);
      
      try {
        // Find and kill tsx main.ts processes
        await execAsync('pkill -f "tsx main.ts"');
        
        // Wait for graceful shutdown
        await new Promise(resolve => setTimeout(resolve, 2000));
        
        // Verify shutdown
        try {
          const response = await fetch('http://localhost:9000/api/status');
          if (response.ok) {
            // Still running, force kill
            await execAsync('pkill -9 -f "tsx main.ts"');
            await new Promise(resolve => setTimeout(resolve, 1000));
          }
        } catch {
          // Good, server is down
        }
        
        return this.createSuccessResult(
          `Continuum daemon system stopped successfully`,
          {
            status: 'stopped',
            interface: 'http://localhost:9000'
          }
        );
        
      } catch (error) {
        // Process might not exist, check if server is actually down
        try {
          await fetch('http://localhost:9000/api/status');
          return this.createErrorResult(`Failed to stop daemon system: ${error instanceof Error ? error.message : String(error)}`);
        } catch {
          // Server is down, stop succeeded
          return this.createSuccessResult(
            `Continuum daemon system stopped`,
            {
              status: 'stopped',
              interface: 'http://localhost:9000'
            }
          );
        }
      }

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Failed to stop daemon system: ${errorMessage}`);
    }
  }
}