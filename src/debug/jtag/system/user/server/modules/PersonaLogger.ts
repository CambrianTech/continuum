/**
 * PersonaLogger - Queued logging subprocess for non-blocking log writes
 *
 * Architecture:
 * - Each persona has ONE logger subprocess
 * - All other subprocesses enqueue log messages (non-blocking)
 * - Logger batches and flushes to files asynchronously
 * - Prevents blocking subprocesses on file I/O
 *
 * Benefits:
 * - Non-blocking: enqueue is instant, file I/O happens in background
 * - Batch writes: flush multiple log lines at once (efficient)
 * - Centralized: one place handles all per-persona logging
 * - Clean separation: logging concern isolated from business logic
 */

import { PersonaContinuousSubprocess } from './PersonaSubprocess';
import type { PersonaUser } from '../PersonaUser';
import { appendFileSync, appendFile, mkdirSync, existsSync } from 'fs';
import { dirname } from 'path';

interface LogEntry {
  readonly filePath: string;
  readonly message: string;
  readonly timestamp: Date;
}

/**
 * PersonaLogger - Non-blocking queued logging for a persona
 *
 * Usage:
 * ```typescript
 * // In other subprocesses
 * this.persona.logger.enqueueLog('hippocampus.log', 'Tick started');
 * ```
 */
export class PersonaLogger extends PersonaContinuousSubprocess {
  private logQueue: LogEntry[] = [];
  private readonly maxLogQueueSize: number = 1000; // Prevent unbounded growth
  private readonly batchSize: number = 50; // Flush up to 50 lines per tick

  constructor(persona: PersonaUser) {
    // Highest priority - logging should be fast and responsive
    super(persona, {
      priority: 'highest',
      name: 'PersonaLogger'
    });
  }

  /**
   * Enqueue a log message (non-blocking, instant return)
   *
   * @param fileName - Log file name (e.g., 'hippocampus.log')
   * @param message - Pre-formatted log message (already has timestamp, etc.)
   */
  enqueueLog(fileName: string, message: string): void {
    // Drop oldest if queue full (prevent memory exhaustion)
    if (this.logQueue.length >= this.maxLogQueueSize) {
      this.logQueue.shift();
      // Log to console as fallback (queue overflow)
      console.warn(`⚠️ [PersonaLogger] Queue full, dropped log: ${fileName}`);
    }

    // Build full path
    const filePath = this.getLogFilePath(fileName);

    this.logQueue.push({
      filePath,
      message,
      timestamp: new Date()
    });
  }

  /**
   * Continuous tick - flush queued logs to files in batches
   */
  protected async tick(): Promise<void> {
    if (this.logQueue.length === 0) {
      return; // Nothing to flush
    }

    // Take up to batchSize entries from queue
    const batch = this.logQueue.splice(0, Math.min(this.batchSize, this.logQueue.length));

    // Group by file path (efficient: batch writes to same file)
    const byFile = new Map<string, string>();

    for (const entry of batch) {
      const existing = byFile.get(entry.filePath) || '';
      byFile.set(entry.filePath, existing + entry.message);
    }

    // Flush each file (async, but sequential per file to maintain order)
    for (const [filePath, content] of byFile) {
      await this.flushToFile(filePath, content);
    }
  }

  /**
   * Flush content to file asynchronously
   */
  private async flushToFile(filePath: string, content: string): Promise<void> {
    try {
      // Ensure directory exists
      const dir = dirname(filePath);
      if (!existsSync(dir)) {
        mkdirSync(dir, { recursive: true });
      }

      // Async append (non-blocking)
      await new Promise<void>((resolve, reject) => {
        appendFile(filePath, content, 'utf8', (err) => {
          if (err) reject(err);
          else resolve();
        });
      });
    } catch (error) {
      // Fallback to console (don't lose logs)
      console.error(`❌ [PersonaLogger] Failed to write to ${filePath}:`, error);
      console.log(`[PersonaLogger] Dropped content:\n${content}`);
    }
  }

  /**
   * Get full log file path for a given file name
   *
   * Format: .continuum/personas/{name}-{id}/logs/{subprocess}.log
   * Example: .continuum/personas/grok-aff84949/logs/hippocampus.log
   *
   * Uses persona.homeDirectory as the single source of truth
   */
  private getLogFilePath(fileName: string): string {
    // Use persona's homeDirectory (already absolute path from SystemPaths)
    return `${this.persona.homeDirectory}/logs/${fileName}`;
  }

  /**
   * Synchronous emergency log (use only for critical errors)
   *
   * This bypasses the queue and writes immediately.
   * Only use when logging MUST happen before process might crash.
   */
  emergencyLog(fileName: string, message: string): void {
    const filePath = this.getLogFilePath(fileName);

    try {
      const dir = dirname(filePath);
      if (!existsSync(dir)) {
        mkdirSync(dir, { recursive: true });
      }
      appendFileSync(filePath, message, 'utf8');
    } catch (error) {
      console.error(`❌ [PersonaLogger] Emergency log failed:`, error);
      console.log(message);
    }
  }

  /**
   * Get current queue statistics
   */
  getQueueStats(): {
    queueSize: number;
    maxQueueSize: number;
    utilizationPercent: number;
  } {
    return {
      queueSize: this.logQueue.length,
      maxQueueSize: this.maxLogQueueSize,
      utilizationPercent: (this.logQueue.length / this.maxLogQueueSize) * 100
    };
  }

  /**
   * Force flush all queued logs immediately (blocking)
   *
   * Use during shutdown to ensure all logs are written.
   */
  async forceFlush(): Promise<void> {
    while (this.logQueue.length > 0) {
      await this.tick();
    }
  }
}
