/**
 * SentinelLogWriter - Non-blocking log writer for sentinel execution
 *
 * Writes FULL output (not truncated) to per-sentinel log directories.
 * Uses async file operations - NEVER blocks the main thread.
 *
 * Directory structure:
 *   .sentinel-workspaces/{handle}/logs/
 *   ├── execution.log      # High-level actions
 *   ├── build-1.log        # Full output from build attempt 1
 *   ├── build-2.log        # Full output from build attempt 2
 *   ├── llm-requests.log   # LLM queries and responses
 *   └── stderr.log         # All stderr output
 *
 * Streaming: Emits events as logs are written for real-time UI.
 *   sentinel:{handle}:log - Each log chunk
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { Events } from '@system/core/shared/Events';

export interface LogWriterConfig {
  /** Unique sentinel handle */
  handle: string;

  /** Base directory for sentinel workspaces */
  baseDir?: string;

  /** Whether to emit events for streaming */
  emitEvents?: boolean;
}

export interface LogChunk {
  stream: string;
  chunk: string;
  timestamp: string;
  sourceType: 'stdout' | 'stderr' | 'info' | 'error';
}

/**
 * Non-blocking log writer for sentinel execution.
 * All writes are async - NEVER blocks caller.
 */
export class SentinelLogWriter {
  private handle: string;
  private logsDir: string;
  private emitEvents: boolean;
  private writeQueue: Map<string, Promise<void>> = new Map();
  private buildCounter: number = 0;

  private constructor(config: LogWriterConfig) {
    this.handle = config.handle;
    const baseDir = config.baseDir ?? '.sentinel-workspaces';
    this.logsDir = path.join(baseDir, config.handle, 'logs');
    this.emitEvents = config.emitEvents ?? true;
  }

  /**
   * Create and initialize a log writer.
   * Creates the logs directory if it doesn't exist.
   */
  static async create(config: LogWriterConfig): Promise<SentinelLogWriter> {
    const writer = new SentinelLogWriter(config);
    await fs.mkdir(writer.logsDir, { recursive: true });
    return writer;
  }

  /**
   * Get the logs directory path.
   */
  get logDirectory(): string {
    return this.logsDir;
  }

  /**
   * Write to the execution log (high-level actions).
   * NON-BLOCKING - returns immediately.
   */
  async writeExecution(message: string): Promise<void> {
    await this.appendToStream('execution', message, 'info');
  }

  /**
   * Start a new build log and return its stream name.
   * Each build attempt gets a separate log file.
   */
  startBuildLog(): string {
    this.buildCounter++;
    return `build-${this.buildCounter}`;
  }

  /**
   * Write build output (stdout/stderr) to the current build log.
   * NON-BLOCKING - returns immediately.
   */
  async writeBuildOutput(stream: string, output: string, sourceType: 'stdout' | 'stderr' = 'stdout'): Promise<void> {
    await this.appendToStream(stream, output, sourceType);
  }

  /**
   * Write LLM request/response to the LLM log.
   * NON-BLOCKING - returns immediately.
   */
  async writeLlmRequest(prompt: string, response: string): Promise<void> {
    const timestamp = new Date().toISOString();
    const entry = `\n${'='.repeat(80)}\n[${timestamp}] LLM REQUEST\n${'='.repeat(80)}\n${prompt}\n\n${'='.repeat(80)}\n[${timestamp}] LLM RESPONSE\n${'='.repeat(80)}\n${response}\n`;
    await this.appendToStream('llm-requests', entry, 'info');
  }

  /**
   * Write error output to stderr log.
   * NON-BLOCKING - returns immediately.
   */
  async writeError(error: string): Promise<void> {
    await this.appendToStream('stderr', error, 'error');
  }

  /**
   * Get the full path to a log file.
   */
  getLogPath(stream: string): string {
    return path.join(this.logsDir, `${stream}.log`);
  }

  /**
   * Read a log file's contents.
   */
  async readLog(stream: string): Promise<string> {
    const logPath = this.getLogPath(stream);
    try {
      return await fs.readFile(logPath, 'utf-8');
    } catch {
      return '';
    }
  }

  /**
   * List all log files for this sentinel.
   */
  async listLogs(): Promise<string[]> {
    try {
      const files = await fs.readdir(this.logsDir);
      return files.filter(f => f.endsWith('.log')).map(f => f.replace('.log', ''));
    } catch {
      return [];
    }
  }

  /**
   * Core append method - NON-BLOCKING via async queue.
   * Ensures writes to the same file are serialized (no interleaving).
   */
  private async appendToStream(stream: string, content: string, sourceType: 'stdout' | 'stderr' | 'info' | 'error'): Promise<void> {
    const logPath = this.getLogPath(stream);
    const timestamp = new Date().toISOString();

    // Emit event for real-time streaming (non-blocking)
    if (this.emitEvents) {
      const event: LogChunk = {
        stream,
        chunk: content,
        timestamp,
        sourceType,
      };
      // Fire-and-forget - don't await event emission
      Events.emit(`sentinel:${this.handle}:log`, event).catch(() => {});
    }

    // Queue writes to same file to prevent interleaving
    const existingWrite = this.writeQueue.get(stream) ?? Promise.resolve();
    const newWrite = existingWrite.then(async () => {
      try {
        await fs.appendFile(logPath, content);
      } catch (e) {
        // Log write failure but don't crash
        console.error(`[SentinelLogWriter] Failed to write to ${logPath}:`, e);
      }
    });

    this.writeQueue.set(stream, newWrite);

    // Don't await - return immediately for non-blocking behavior
    // The write will complete in the background
  }

  /**
   * Wait for all pending writes to complete.
   * Call this before sentinel completion to ensure all logs are flushed.
   */
  async flush(): Promise<void> {
    const pending = Array.from(this.writeQueue.values());
    await Promise.all(pending);
    this.writeQueue.clear();
  }
}

/**
 * Factory function for creating log writers.
 */
export async function createSentinelLogWriter(handle: string): Promise<SentinelLogWriter> {
  return SentinelLogWriter.create({ handle });
}
