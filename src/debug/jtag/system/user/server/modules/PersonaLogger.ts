/**
 * PersonaLogger - Wrapper around Logger.ts for persona-specific logging
 *
 * Architecture:
 * - Wraps Logger.ts to get CLEAN mode (fresh logs per session)
 * - Creates ComponentLogger instances for each log file
 * - Uses Logger.ts queuing and batching infrastructure
 * - Prevents blocking subprocesses on file I/O
 *
 * Benefits:
 * - Automatic log cleaning (via Logger.ts FileMode.CLEAN)
 * - Non-blocking: Logger.ts handles async writes
 * - Consistent behavior with all other system logs
 */

import { PersonaContinuousSubprocess } from './PersonaSubprocess';
import type { PersonaUser } from '../PersonaUser';
import { Logger, FileMode, type ComponentLogger } from '../../../core/logging/Logger';

interface LoggerMap {
  [fileName: string]: ComponentLogger;
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
  private loggers: LoggerMap = {};

  constructor(persona: PersonaUser) {
    // Highest priority - logging should be fast and responsive
    super(persona, {
      priority: 'highest',
      name: 'PersonaLogger'
    });
  }

  /**
   * Enqueue a log message (uses Logger.ts infrastructure)
   *
   * @param fileName - Log file name (e.g., 'hippocampus.log', 'cognition.log')
   * @param message - Pre-formatted log message (already has timestamp, etc.)
   */
  enqueueLog(fileName: string, message: string): void {
    // Get or create logger for this file
    if (!this.loggers[fileName]) {
      const filePath = this.getLogFilePath(fileName);
      const componentName = `${this.persona.displayName}:${fileName.replace('.log', '')}`;
      this.loggers[fileName] = Logger.createWithFile(componentName, filePath, FileMode.CLEAN);
    }

    // Write directly (Logger.ts handles queuing and async writes)
    this.loggers[fileName].writeRaw(message);
  }

  /**
   * Continuous tick - no-op (Logger.ts handles all flushing)
   */
  protected async tick(): Promise<void> {
    // Logger.ts handles all async writes and batching
    // Nothing to do here
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
   * Emergency log (fallback to console)
   */
  emergencyLog(fileName: string, message: string): void {
    console.error(`❌ [PersonaLogger:${this.persona.displayName}] ${fileName}: ${message}`);
  }

  /**
   * Get current queue statistics (no-op - Logger.ts handles queue)
   */
  getQueueStats(): { queueSize: number; maxQueueSize: number; utilizationPercent: number } {
    return { queueSize: 0, maxQueueSize: 0, utilizationPercent: 0 };
  }

  /**
   * Force flush all queued logs (Logger.ts handles this)
   */
  async forceFlush(): Promise<void> {
    // Logger.ts handles flushing
  }
}
