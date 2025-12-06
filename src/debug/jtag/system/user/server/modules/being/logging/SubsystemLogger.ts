/**
 * SubsystemLogger - Persona cognitive logging (thin wrapper around Logger)
 *
 * Provides file-based logging per persona and subsystem (Mind/Body/Soul/CNS).
 * This is now just a convenience wrapper around the core Logger system.
 *
 * Directory structure:
 * .continuum/personas/{uniqueId}/logs/
 *   ├── mind.log       (cognition, state tracking)
 *   ├── body.log       (action, execution, tools)
 *   ├── soul.log       (memory, learning, genome)
 *   └── cns.log        (orchestration, coordination)
 *
 * Usage:
 * ```typescript
 * import { SubsystemLogger } from './SubsystemLogger';
 *
 * class PersonaMind {
 *   private logger: SubsystemLogger;
 *
 *   constructor(personaUser: PersonaUserForMind) {
 *     this.logger = new SubsystemLogger('mind', personaUser.id, personaUser.entity.uniqueId);
 *     this.logger.info('Mind initialized');
 *   }
 * }
 * ```
 */

import * as path from 'path';
import type { UUID } from '../../../../../core/types/CrossPlatformUUID';
import { SystemPaths } from '../../../../../core/config/SystemPaths';
import { Logger, FileMode } from '../../../../../core/logging/Logger';
import type { ComponentLogger } from '../../../../../core/logging/Logger';

export type Subsystem = 'mind' | 'body' | 'soul' | 'cns';

export interface LoggerConfig {
  /**
   * Custom log directory - can be:
   * - Absolute path: '/full/path/to/logs'
   * - Relative path: 'personas/name-id/logs' (resolved relative to .continuum/)
   * Default: .continuum/personas/{uniqueId}/logs
   */
  logDir?: string;

  /**
   * File mode: CLEAN (default - start fresh), APPEND (keep old logs), ARCHIVE (future)
   */
  mode?: FileMode;
}

/**
 * SubsystemLogger - Thin wrapper around Logger for persona cognitive logs
 *
 * All actual logging logic is handled by the core Logger system.
 * This just provides a persona-specific interface with subsystem names.
 */
export class SubsystemLogger {
  private readonly subsystem: Subsystem;
  private readonly personaId: UUID;
  private readonly uniqueId: string;
  private readonly logger: ComponentLogger;

  constructor(
    subsystem: Subsystem,
    personaId: UUID,
    uniqueId: string,
    config: LoggerConfig = {}
  ) {
    this.subsystem = subsystem;
    this.personaId = personaId;
    this.uniqueId = uniqueId;

    // Determine log file path
    const logDir = config.logDir
      ? (path.isAbsolute(config.logDir) ? config.logDir : path.join(SystemPaths.root, config.logDir))
      : SystemPaths.logs.personas(uniqueId);

    const logFilePath = path.join(logDir, `${subsystem}.log`);
    // Persona logs default to CLEAN (start fresh per session)
    // Caller can override by passing mode in config
    const mode = config.mode ?? FileMode.CLEAN;

    // Create logger using core Logger system
    const componentName = `${uniqueId}:${subsystem}`;
    this.logger = Logger.createWithFile(componentName, logFilePath, mode);
  }

  // Delegate all logging methods to ComponentLogger

  debug(message: string, ...args: unknown[]): void {
    this.logger.debug(message, ...args);
  }

  info(message: string, ...args: unknown[]): void {
    this.logger.info(message, ...args);
  }

  warn(message: string, ...args: unknown[]): void {
    this.logger.warn(message, ...args);
  }

  error(message: string, ...args: unknown[]): void {
    this.logger.error(message, ...args);
  }

  /**
   * Conditional debug logging (only executes if debug level enabled)
   */
  debugIf(messageFn: () => [string, ...any[]]): void {
    this.logger.debugIf(messageFn);
  }

  /**
   * Close logger (handled by Logger.shutdown(), but provided for compatibility)
   */
  close(): void {
    // No-op - Logger handles cleanup on shutdown
  }

  /**
   * Get log file path (for debugging)
   */
  getLogFilePath(): string {
    return path.join(
      SystemPaths.logs.personas(this.uniqueId),
      `${this.subsystem}.log`
    );
  }

  /**
   * Enqueue log to specific file (used by persona modules)
   * @param fileName - Log file name (e.g., 'genome.log', 'training.log')
   * @param message - Log message to write
   */
  enqueueLog(fileName: string, message: string): void {
    // Determine full path for the log file
    const logDir = SystemPaths.logs.personas(this.uniqueId);
    const logFilePath = path.join(logDir, fileName);

    // Create a ComponentLogger for this file and write the raw message
    const fileLogger = Logger.createWithFile(
      `${this.uniqueId}:${fileName.replace('.log', '')}`,
      logFilePath,
      FileMode.CLEAN
    );
    fileLogger.writeRaw(message + '\n');
  }
}
