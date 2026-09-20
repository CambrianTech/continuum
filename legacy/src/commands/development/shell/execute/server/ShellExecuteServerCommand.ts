/**
 * Shell Execute Command - Server Implementation
 *
 * Executes whitelisted shell commands safely using Node.js child_process.
 * Provides structured output for AI agents and PersonaUsers.
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { ShellExecuteCommand } from '../shared/ShellExecuteCommand';
import type { ShellExecuteParams, ShellExecuteResult } from '../shared/ShellExecuteTypes';
import { spawn } from 'child_process';

export class ShellExecuteServerCommand extends ShellExecuteCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  /**
   * Execute whitelisted shell command with safety constraints
   */
  async execute(params: ShellExecuteParams): Promise<ShellExecuteResult> {
    const startTime = Date.now();

    // Validate parameters
    const validation = this.validateParams(params);
    if (!validation.valid) {
      console.error(`❌ SHELL/EXECUTE: Validation failed - ${validation.error}`);
      return this.formatErrorResult(params, validation.error ?? 'Invalid parameters');
    }

    // Normalize and sanitize parameters
    const normalizedParams = this.normalizeParams(params);
    const { command, args = [], timeout, maxOutputSize, cwd, env } = normalizedParams;

    const executedCommand = `${command} ${args.join(' ')}`;
    console.log(`🐚 SHELL/EXECUTE: Running "${executedCommand}"${cwd ? ` in ${cwd}` : ''}`);

    try {
      // Execute command
      const result = await this.executeShellCommand(
        command,
        args,
        timeout!,
        maxOutputSize!,
        cwd,
        env
      );

      const executionTimeMs = Date.now() - startTime;

      console.log(
        `✅ SHELL/EXECUTE: Completed "${executedCommand}" in ${executionTimeMs}ms (exit ${result.exitCode})`
      );

      return this.formatSuccessResult(
        params,
        result.stdout,
        result.stderr,
        result.exitCode,
        executedCommand,
        executionTimeMs,
        result.truncated
      );

    } catch (error) {
      const executionTimeMs = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);

      console.error(`❌ SHELL/EXECUTE: Command "${executedCommand}" failed after ${executionTimeMs}ms:`, errorMessage);

      return this.formatErrorResult(params, errorMessage, executedCommand);
    }
  }

  /**
   * Execute shell command using Node.js child_process.spawn
   * Returns structured output with stdout, stderr, exit code
   */
  private executeShellCommand(
    command: string,
    args: string[],
    timeout: number,
    maxOutputSize: number,
    cwd?: string,
    env?: Record<string, string>
  ): Promise<{ stdout: string; stderr: string; exitCode: number; truncated: boolean }> {
    return new Promise((resolve, reject) => {
      let stdout = '';
      let stderr = '';
      let truncated = false;
      let outputSize = 0;

      // Spawn child process
      const child = spawn(command, args, {
        cwd: cwd ?? process.cwd(),
        env: env ? { ...process.env, ...env } : process.env,
        shell: false, // Don't use shell for security (prevents injection)
        timeout
      });

      // Collect stdout
      child.stdout.on('data', (data: Buffer) => {
        const chunk = data.toString();
        outputSize += chunk.length;

        if (outputSize <= maxOutputSize) {
          stdout += chunk;
        } else if (!truncated) {
          stdout += '\n... (output truncated due to size limit)\n';
          truncated = true;
        }
      });

      // Collect stderr
      child.stderr.on('data', (data: Buffer) => {
        const chunk = data.toString();
        outputSize += chunk.length;

        if (outputSize <= maxOutputSize) {
          stderr += chunk;
        } else if (!truncated) {
          stderr += '\n... (output truncated due to size limit)\n';
          truncated = true;
        }
      });

      // Handle process exit
      child.on('close', (code: number | null) => {
        resolve({
          stdout: stdout.trim(),
          stderr: stderr.trim(),
          exitCode: code ?? -1,
          truncated
        });
      });

      // Handle errors (spawn failures, timeouts, etc.)
      child.on('error', (error: Error) => {
        reject(new Error(`Failed to execute command: ${error.message}`));
      });

      // Handle timeout
      setTimeout(() => {
        if (!child.killed) {
          child.kill('SIGTERM');
          reject(new Error(`Command timed out after ${timeout}ms`));
        }
      }, timeout);
    });
  }
}
