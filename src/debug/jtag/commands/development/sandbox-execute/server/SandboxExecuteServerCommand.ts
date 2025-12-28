/**
 * Sandbox Execute Command - Server Implementation
 *
 * Executes AI-generated commands from persona sandboxes using npx tsx.
 * No main system recompile needed - dynamic TypeScript execution.
 *
 * Security constraints:
 * - Path must be within .continuum/personas/{uniqueId}/sandbox/
 * - Only executes files named *ServerCommand.ts
 * - Timeout protection
 * - Output size limits
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { SandboxExecuteParams, SandboxExecuteResult } from '../shared/SandboxExecuteTypes';
import { createSandboxExecuteResult } from '../shared/SandboxExecuteTypes';
import { SystemPaths } from '@system/core/config/SystemPaths';
import { spawn } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

export class SandboxExecuteServerCommand extends CommandBase<SandboxExecuteParams, SandboxExecuteResult> {

  private static readonly DEFAULT_TIMEOUT = 30000; // 30 seconds
  private static readonly MAX_OUTPUT_SIZE = 100000; // 100KB

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    // Note: Command name is derived from directory structure by CommandDaemon
    super('sandbox-execute', context, subpath, commander);
  }

  /**
   * Execute sandbox command with safety constraints
   */
  async execute(params: SandboxExecuteParams): Promise<SandboxExecuteResult> {
    const startTime = Date.now();

    try {
      // 1. Validate command path
      const validation = this.validateCommandPath(params.commandPath);
      if (!validation.valid) {
        return createSandboxExecuteResult(this.context, this.context.uuid, {
          success: false,
          error: validation.error
        });
      }

      // 2. Find the server command file
      const serverFile = this.findServerCommandFile(params.commandPath);
      if (!serverFile) {
        return createSandboxExecuteResult(this.context, this.context.uuid, {
          success: false,
          error: `No *ServerCommand.ts file found in ${params.commandPath}/server/`
        });
      }

      console.log(`🔧 SANDBOX-EXECUTE: Running ${serverFile}`);

      // 3. Execute with npx tsx
      const result = await this.executeTsx(
        serverFile,
        params.params || {},
        params.timeout || SandboxExecuteServerCommand.DEFAULT_TIMEOUT
      );

      const executionTimeMs = Date.now() - startTime;

      // 4. Parse output if JSON
      let parsedOutput: unknown = undefined;
      if (result.stdout) {
        try {
          parsedOutput = JSON.parse(result.stdout);
        } catch {
          // Not JSON, use raw stdout
          parsedOutput = result.stdout;
        }
      }

      console.log(`✅ SANDBOX-EXECUTE: Completed in ${executionTimeMs}ms (exit ${result.exitCode})`);

      return createSandboxExecuteResult(this.context, this.context.uuid, {
        success: result.exitCode === 0,
        output: parsedOutput,
        stdout: result.stdout,
        stderr: result.stderr,
        executionTimeMs,
        error: result.exitCode !== 0 ? `Exit code: ${result.exitCode}` : undefined
      });

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ SANDBOX-EXECUTE: Failed:`, errorMessage);

      return createSandboxExecuteResult(this.context, this.context.uuid, {
        success: false,
        error: errorMessage,
        executionTimeMs: Date.now() - startTime
      });
    }
  }

  /**
   * Validate that command path is within allowed sandbox directories
   */
  private validateCommandPath(commandPath: string): { valid: boolean; error?: string } {
    const rootPath = SystemPaths.root;

    // Resolve to absolute path
    const absolutePath = path.isAbsolute(commandPath)
      ? commandPath
      : path.resolve(rootPath, commandPath);

    // Check path exists
    if (!fs.existsSync(absolutePath)) {
      return { valid: false, error: `Path does not exist: ${absolutePath}` };
    }

    // Security: Must be within .continuum/personas/*/sandbox/ OR test directories
    const continuumPath = path.join(rootPath, '.continuum');
    const sandboxPattern = /\.continuum[\/\\]personas[\/\\][^\/\\]+[\/\\]sandbox/;

    const isInSandbox = sandboxPattern.test(absolutePath);
    const isInContinuum = absolutePath.startsWith(continuumPath);

    // For testing, allow paths in /tmp or explicit test directories
    const isTestPath = absolutePath.startsWith('/tmp') ||
                       absolutePath.includes('/test/') ||
                       absolutePath.includes('/tests/');

    if (!isInSandbox && !isTestPath) {
      return {
        valid: false,
        error: `Security: Path must be within .continuum/personas/*/sandbox/ (got: ${absolutePath})`
      };
    }

    return { valid: true };
  }

  /**
   * Find the *ServerCommand.ts file in the command directory
   */
  private findServerCommandFile(commandPath: string): string | null {
    const rootPath = SystemPaths.root;
    const absolutePath = path.isAbsolute(commandPath)
      ? commandPath
      : path.resolve(rootPath, commandPath);

    const serverDir = path.join(absolutePath, 'server');

    if (!fs.existsSync(serverDir)) {
      return null;
    }

    const files = fs.readdirSync(serverDir);
    const serverFile = files.find(f => f.endsWith('ServerCommand.ts'));

    if (!serverFile) {
      return null;
    }

    return path.join(serverDir, serverFile);
  }

  /**
   * Execute TypeScript file with npx tsx
   */
  private executeTsx(
    filePath: string,
    params: Record<string, unknown>,
    timeout: number
  ): Promise<{ stdout: string; stderr: string; exitCode: number }> {
    return new Promise((resolve, reject) => {
      let stdout = '';
      let stderr = '';
      let outputSize = 0;

      // Pass params as JSON string argument
      const paramsJson = JSON.stringify(params);

      console.log(`🔧 SANDBOX-EXECUTE: npx tsx ${filePath} '${paramsJson}'`);

      // Spawn npx tsx with the file and params
      const child = spawn('npx', ['tsx', filePath, paramsJson], {
        cwd: path.dirname(filePath),
        env: {
          ...process.env,
          // Add any sandbox-specific env vars here
          SANDBOX_EXECUTION: 'true'
        },
        shell: false,
        timeout
      });

      // Collect stdout
      child.stdout.on('data', (data: Buffer) => {
        const chunk = data.toString();
        outputSize += chunk.length;

        if (outputSize <= SandboxExecuteServerCommand.MAX_OUTPUT_SIZE) {
          stdout += chunk;
        }
      });

      // Collect stderr
      child.stderr.on('data', (data: Buffer) => {
        const chunk = data.toString();
        outputSize += chunk.length;

        if (outputSize <= SandboxExecuteServerCommand.MAX_OUTPUT_SIZE) {
          stderr += chunk;
        }
      });

      // Handle process exit
      child.on('close', (code: number | null) => {
        resolve({
          stdout: stdout.trim(),
          stderr: stderr.trim(),
          exitCode: code ?? -1
        });
      });

      // Handle errors
      child.on('error', (error: Error) => {
        reject(new Error(`Failed to execute tsx: ${error.message}`));
      });

      // Handle timeout
      setTimeout(() => {
        if (!child.killed) {
          child.kill('SIGTERM');
          reject(new Error(`Sandbox command timed out after ${timeout}ms`));
        }
      }, timeout);
    });
  }
}
