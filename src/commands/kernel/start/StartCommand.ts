/**
 * Start Command - Start the Continuum daemon system
 */

import { DirectCommand } from '../../core/direct-command/DirectCommand';
import { CommandDefinition, ContinuumContext, CommandResult } from '../../core/base-command/BaseCommand';

export class StartCommand extends DirectCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'start',
      category: 'kernel',
      icon: '🚀',
      description: 'Start the Continuum daemon system',
      parameters: {},
      examples: [
        { 
          description: 'Start daemon system', 
          command: `{}` 
        }
      ],
      usage: 'Start the Continuum daemon operating system'
    };
  }

  protected static async executeOperation(_params: any = {}, _context?: ContinuumContext): Promise<CommandResult> {
    try {
      // Check if daemons are already running
      try {
        const response = await fetch('http://localhost:9000/api/status');
        if (response.ok) {
          const status = await response.json();
          return this.createSuccessResult(
            `Continuum daemons already running`,
            {
              status: 'already_running',
              daemons: status.daemons,
              interface: 'http://localhost:9000',
              connections: status.connections
            }
          );
        }
      } catch {
        // Server not running, continue with start
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
      
      // Wait a moment for startup
      await new Promise(resolve => setTimeout(resolve, 2000));
      
      // Verify startup
      try {
        const response = await fetch('http://localhost:9000/api/status');
        if (response.ok) {
          const status = await response.json();
          return this.createSuccessResult(
            `Continuum daemon system started successfully`,
            {
              status: 'started',
              daemons: status.daemons,
              interface: 'http://localhost:9000',
              pid: childProcess.pid,
              connections: status.connections
            }
          );
        } else {
          throw new Error('Startup verification failed');
        }
      } catch (error) {
        return this.createErrorResult(`Daemon startup failed: ${error instanceof Error ? error.message : String(error)}`);
      }

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Failed to start daemon system: ${errorMessage}`);
    }
  }
}