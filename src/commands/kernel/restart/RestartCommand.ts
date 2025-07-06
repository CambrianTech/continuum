/**
 * Restart Command - Restart the Continuum daemon system
 */

import { DirectCommand } from '../../core/direct-command/DirectCommand.js';
import { CommandDefinition, CommandContext, CommandResult } from '../../core/base-command/BaseCommand.js';

export class RestartCommand extends DirectCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'restart',
      category: 'kernel',
      icon: '🔄',
      description: 'Restart the Continuum daemon system',
      parameters: {},
      examples: [
        { 
          description: 'Restart daemon system', 
          command: `{}` 
        }
      ],
      usage: 'Restart the Continuum daemon operating system'
    };
  }

  protected static async executeOperation(_params: any = {}, _context?: CommandContext): Promise<CommandResult> {
    try {
      let wasRunning = false;
      
      // Check if daemons are currently running
      try {
        const response = await fetch('http://localhost:9000/api/status');
        wasRunning = response.ok;
      } catch {
        wasRunning = false;
      }

      // Stop the daemon system if running
      if (wasRunning) {
        const { exec } = await import('child_process');
        const { promisify } = await import('util');
        const execAsync = promisify(exec);
        
        try {
          await execAsync('pkill -f "tsx main.ts"');
          await new Promise(resolve => setTimeout(resolve, 2000));
        } catch {
          // Process might not exist, continue
        }
      }

      // Start the daemon system
      const { spawn } = await import('child_process');
      const path = await import('path');
      
      // Find continuum root directory
      const continuumRoot = process.cwd();
      const mainScript = path.join(continuumRoot, 'main.ts');
      
      // Start daemons in background
      const childProcess = spawn('npx', ['tsx', mainScript], {
        detached: true,
        stdio: 'ignore',
        cwd: continuumRoot
      });
      
      // Detach the process so it runs independently
      childProcess.unref();
      
      // Wait for startup
      await new Promise(resolve => setTimeout(resolve, 3000));
      
      // Verify startup
      try {
        const response = await fetch('http://localhost:9000/api/status');
        if (response.ok) {
          const status = await response.json();
          return this.createSuccessResult(
            `Continuum daemon system restarted successfully`,
            {
              status: 'restarted',
              wasRunning,
              daemons: status.daemons,
              interface: 'http://localhost:9000',
              pid: childProcess.pid,
              connections: status.connections
            }
          );
        } else {
          throw new Error('Restart verification failed');
        }
      } catch (error) {
        return this.createErrorResult(`Daemon restart failed: ${error instanceof Error ? error.message : String(error)}`);
      }

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Failed to restart daemon system: ${errorMessage}`);
    }
  }
}