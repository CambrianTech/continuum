/**
 * CLI command execution and testing
 */

import { spawn, SpawnOptions } from 'child_process';
import { CLITestResult } from './types';

export class CLIRunner {
  
  async run(command: string, args: string[] = [], options: {
    cwd?: string;
    timeout?: number;
    env?: Record<string, string>;
  } = {}): Promise<CLITestResult> {
    
    const startTime = Date.now();
    
    return new Promise((resolve, reject) => {
      const spawnOptions: SpawnOptions = {
        cwd: options.cwd,
        env: { ...process.env, ...options.env },
        stdio: ['pipe', 'pipe', 'pipe']
      };

      const child = spawn(command, args, spawnOptions);
      
      let stdout = '';
      let stderr = '';
      
      child.stdout?.on('data', (data) => {
        stdout += data.toString();
      });
      
      child.stderr?.on('data', (data) => {
        stderr += data.toString();
      });

      // Handle timeout
      const timeout = options.timeout || 30000;
      const timeoutId = setTimeout(() => {
        child.kill('SIGTERM');
        reject(new Error(`Command timed out after ${timeout}ms`));
      }, timeout);

      child.on('close', (code) => {
        clearTimeout(timeoutId);
        const duration = Date.now() - startTime;
        
        resolve({
          command: `${command} ${args.join(' ')}`,
          exitCode: code || 0,
          stdout: stdout.trim(),
          stderr: stderr.trim(),
          duration
        });
      });

      child.on('error', (error) => {
        clearTimeout(timeoutId);
        reject(error);
      });
    });
  }

  async testPythonCLI(scriptPath: string, args: string[] = [], options: {
    cwd?: string;
    timeout?: number;
    pythonExecutable?: string;
  } = {}): Promise<CLITestResult> {
    const python = options.pythonExecutable || 'python3';
    return this.run(python, [scriptPath, ...args], options);
  }

  async testNodeCLI(scriptPath: string, args: string[] = [], options: {
    cwd?: string;
    timeout?: number;
  } = {}): Promise<CLITestResult> {
    return this.run('node', [scriptPath, ...args], options);
  }

  async testCommand(commandLine: string, options: {
    cwd?: string;
    timeout?: number;
    shell?: boolean;
  } = {}): Promise<CLITestResult> {
    if (options.shell) {
      return this.run('sh', ['-c', commandLine], options);
    } else {
      const [command, ...args] = commandLine.split(' ');
      return this.run(command, args, options);
    }
  }
}