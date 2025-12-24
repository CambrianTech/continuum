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

import * as fs from 'fs';
import * as path from 'path';
import { PersonaContinuousSubprocess } from './PersonaSubprocess';
import type { PersonaUser } from '../PersonaUser';
import { Logger, FileMode, type ComponentLogger } from '../../../core/logging/Logger';
import { LoggingConfig } from '../../../core/logging/LoggingConfig';

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
  private logsDir: string;

  constructor(persona: PersonaUser) {
    // Highest priority - logging should be fast and responsive
    super(persona, {
      priority: 'highest',
      name: 'PersonaLogger'
    });

    // Compute logs directory path
    this.logsDir = path.join(persona.homeDirectory, 'logs');

    // Clean all log files at startup (CLEAN mode)
    this.cleanLogDirectory();
  }

  /**
   * Clean all log files in persona's logs directory at startup
   * This ensures fresh logs per session even for log categories
   * that might not be written to during the session.
   */
  private cleanLogDirectory(): void {
    try {
      if (!fs.existsSync(this.logsDir)) {
        // No logs directory yet - will be created on first write
        return;
      }

      const files = fs.readdirSync(this.logsDir);
      for (const file of files) {
        if (file.endsWith('.log')) {
          const filePath = path.join(this.logsDir, file);
          // Truncate to empty (synchronous since this is startup)
          fs.writeFileSync(filePath, '', 'utf-8');
        }
      }
    } catch (error) {
      // Non-fatal - logging will still work, just won't be cleaned
      console.warn(`[PersonaLogger:${this.persona.displayName}] Failed to clean logs:`, error);
    }
  }

  /**
   * Enqueue a log message (uses Logger.ts infrastructure)
   *
   * @param fileName - Log file name (e.g., 'hippocampus.log', 'cognition.log')
   * @param message - Pre-formatted log message (already has timestamp, etc.)
   */
  enqueueLog(fileName: string, message: string): void {
    // Extract category from fileName (e.g., 'cognition.log' -> 'cognition')
    const category = fileName.replace(/\.log$/, '');

    // Check if logging is enabled for this persona + category
    if (!LoggingConfig.isEnabled(this.persona.displayName, category)) {
      return; // Early exit - logging disabled for this persona/category
    }

    // Get or create logger for this file
    if (!this.loggers[fileName]) {
      const logCategory = `logs/${category}`;
      const componentName = `${this.persona.displayName}:${category}`;
      // Use persona's home directory as logRoot (not system log dir)
      this.loggers[fileName] = Logger.create(componentName, logCategory, this.persona.homeDirectory);
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
