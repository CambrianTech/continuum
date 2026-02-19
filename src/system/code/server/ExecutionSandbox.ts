/**
 * ExecutionSandbox - Process-isolated code execution for coding agents
 *
 * Runs commands in a restricted child process with:
 * - Restricted PATH (only node, npx, tsc)
 * - Timeout enforcement (SIGTERM on timeout, SIGKILL after grace period)
 * - Output capture with size limits
 * - Working directory scoped to persona workspace
 * - Environment variable isolation
 *
 * Based on the existing SandboxExecuteServerCommand spawn pattern,
 * extracted as a reusable utility for Phase 4A sandboxing.
 */

import { spawn, type ChildProcess } from 'child_process';
import * as path from 'path';
import { Logger } from '../../core/logging/Logger';
import type { UUID } from '../../core/types/CrossPlatformUUID';

const log = Logger.create('ExecutionSandbox', 'code');

// ────────────────────────────────────────────────────────────
// Types
// ────────────────────────────────────────────────────────────

export interface SandboxConfig {
  /** Command to execute (e.g., 'npx', 'node', 'tsc') */
  readonly command: string;

  /** Command arguments */
  readonly args: readonly string[];

  /** Working directory — must be within persona workspace */
  readonly cwd: string;

  /** Maximum execution time in milliseconds */
  readonly timeoutMs: number;

  /** Maximum combined stdout+stderr size in bytes */
  readonly maxOutputBytes: number;

  /** Additional environment variables (merged with restricted base) */
  readonly env?: Readonly<Record<string, string>>;

  /** Persona executing this command (for audit logging) */
  readonly personaId: UUID;
}

export interface SandboxResult {
  readonly success: boolean;
  readonly exitCode: number;
  readonly stdout: string;
  readonly stderr: string;
  readonly durationMs: number;
  readonly truncated: boolean;
  readonly timedOut: boolean;
  readonly error?: string;
}

// ────────────────────────────────────────────────────────────
// Defaults
// ────────────────────────────────────────────────────────────

const DEFAULT_TIMEOUT_MS = 30_000;
const DEFAULT_MAX_OUTPUT_BYTES = 102_400; // 100KB
const KILL_GRACE_PERIOD_MS = 5_000;

/** Restricted set of allowed commands */
const ALLOWED_COMMANDS = new Set(['node', 'npx', 'tsc', 'npm']);

/** Restricted PATH — only common binary locations (includes Homebrew for macOS) */
const RESTRICTED_PATH = [
  '/opt/homebrew/bin',   // macOS Apple Silicon Homebrew
  '/usr/local/bin',      // macOS Intel Homebrew / standard
  '/usr/bin',
  '/bin',
].join(path.delimiter);

// ────────────────────────────────────────────────────────────
// Sandbox
// ────────────────────────────────────────────────────────────

export class ExecutionSandbox {
  /**
   * Execute a command in a sandboxed child process.
   */
  async execute(config: SandboxConfig): Promise<SandboxResult> {
    const startTime = performance.now();

    // Validate command is in allowlist
    const baseCommand = path.basename(config.command);
    if (!ALLOWED_COMMANDS.has(baseCommand)) {
      return {
        success: false,
        exitCode: -1,
        stdout: '',
        stderr: '',
        durationMs: 0,
        truncated: false,
        timedOut: false,
        error: `Command '${baseCommand}' is not in the sandbox allowlist. Allowed: ${[...ALLOWED_COMMANDS].join(', ')}`,
      };
    }

    const timeoutMs = config.timeoutMs || DEFAULT_TIMEOUT_MS;
    const maxOutputBytes = config.maxOutputBytes || DEFAULT_MAX_OUTPUT_BYTES;

    log.debug(`Sandbox exec: ${config.command} ${config.args.join(' ')} (timeout: ${timeoutMs}ms, persona: ${config.personaId})`);

    return new Promise<SandboxResult>((resolve) => {
      let stdout = '';
      let stderr = '';
      let outputSize = 0;
      let truncated = false;
      let timedOut = false;
      let child: ChildProcess;

      try {
        child = spawn(config.command, [...config.args], {
          cwd: config.cwd,
          env: {
            PATH: RESTRICTED_PATH,
            NODE_ENV: 'sandbox',
            HOME: config.cwd,
            SANDBOX_EXECUTION: 'true',
            PERSONA_ID: config.personaId,
            ...config.env,
          },
          shell: false,
          stdio: ['ignore', 'pipe', 'pipe'], // No stdin
        });
      } catch (error) {
        const durationMs = performance.now() - startTime;
        resolve({
          success: false,
          exitCode: -1,
          stdout: '',
          stderr: '',
          durationMs,
          truncated: false,
          timedOut: false,
          error: `Failed to spawn: ${error instanceof Error ? error.message : String(error)}`,
        });
        return;
      }

      // Collect stdout with size limit
      child.stdout?.on('data', (data: Buffer) => {
        const chunk = data.toString();
        outputSize += chunk.length;
        if (outputSize <= maxOutputBytes) {
          stdout += chunk;
        } else {
          truncated = true;
        }
      });

      // Collect stderr with size limit
      child.stderr?.on('data', (data: Buffer) => {
        const chunk = data.toString();
        outputSize += chunk.length;
        if (outputSize <= maxOutputBytes) {
          stderr += chunk;
        } else {
          truncated = true;
        }
      });

      // Timeout: SIGTERM first, then SIGKILL after grace period
      const timeoutHandle = setTimeout(() => {
        timedOut = true;
        log.warn(`Sandbox timeout: killing process after ${timeoutMs}ms`);
        child.kill('SIGTERM');

        setTimeout(() => {
          if (!child.killed) {
            child.kill('SIGKILL');
          }
        }, KILL_GRACE_PERIOD_MS);
      }, timeoutMs);

      // Handle process exit
      child.on('close', (code: number | null) => {
        clearTimeout(timeoutHandle);
        const durationMs = performance.now() - startTime;

        log.debug(`Sandbox done: exit=${code ?? -1}, duration=${Math.round(durationMs)}ms, output=${outputSize}b`);

        resolve({
          success: !timedOut && code === 0,
          exitCode: code ?? -1,
          stdout: stdout.trim(),
          stderr: stderr.trim(),
          durationMs,
          truncated,
          timedOut,
          error: timedOut ? `Timed out after ${timeoutMs}ms` : undefined,
        });
      });

      // Handle spawn errors
      child.on('error', (error: Error) => {
        clearTimeout(timeoutHandle);
        const durationMs = performance.now() - startTime;

        resolve({
          success: false,
          exitCode: -1,
          stdout: stdout.trim(),
          stderr: stderr.trim(),
          durationMs,
          truncated,
          timedOut: false,
          error: `Spawn error: ${error.message}`,
        });
      });
    });
  }
}
