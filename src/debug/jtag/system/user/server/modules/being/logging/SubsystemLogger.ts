/**
 * SubsystemLogger - Persona cognitive logging (thin wrapper around Logger)
 *
 * Provides file-based logging per persona and subsystem (neuroanatomy-inspired).
 * This is now just a convenience wrapper around the core Logger system.
 *
 * Directory structure:
 * .continuum/personas/{uniqueId}/logs/
 *   ├── prefrontal.log   (cognition, planning, state tracking)
 *   ├── motor-cortex.log (action, execution, tools)
 *   ├── limbic.log       (memory, learning, emotion, genome)
 *   ├── hippocampus.log  (memory consolidation)
 *   └── cns.log          (orchestration, coordination)
 *
 * Usage:
 * ```typescript
 * import { SubsystemLogger } from './SubsystemLogger';
 *
 * class PrefrontalCortex {
 *   private logger: SubsystemLogger;
 *
 *   constructor(personaUser: PersonaUserForPrefrontal) {
 *     this.logger = new SubsystemLogger('prefrontal', personaUser.id, personaUser.entity.uniqueId);
 *     this.logger.info('Prefrontal cortex initialized');
 *   }
 * }
 * ```
 */

import * as path from 'path';
import type { UUID } from '../../../../../core/types/CrossPlatformUUID';
import { SystemPaths } from '../../../../../core/config/SystemPaths';
import { Logger, FileMode } from '../../../../../core/logging/Logger';
import type { ComponentLogger } from '../../../../../core/logging/Logger';
import { LoggingConfig } from '../../../../../core/logging/LoggingConfig';

export type Subsystem = 'prefrontal' | 'motor-cortex' | 'limbic' | 'hippocampus' | 'cns' | 'rust-cognition';

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
  private readonly logRoot: string;
  private readonly logRootIsLogsDir: boolean;  // true if logRoot is already the logs directory

  constructor(
    subsystem: Subsystem,
    personaId: UUID,
    uniqueId: string,
    config: LoggerConfig = {}
  ) {
    this.subsystem = subsystem;
    this.personaId = personaId;
    this.uniqueId = uniqueId;

    const componentName = `${uniqueId}:${subsystem}`;

    // When config.logDir is provided, it's already the logs directory
    // so category is just the subsystem name. Otherwise, we default to
    // persona home and add logs/ prefix to category.
    if (config.logDir) {
      // config.logDir IS the logs directory - use it as logRoot, subsystem as category
      this.logRootIsLogsDir = true;
      this.logRoot = path.isAbsolute(config.logDir)
        ? config.logDir
        : path.join(SystemPaths.root, config.logDir);
      this.logger = Logger.create(componentName, subsystem, this.logRoot);
    } else {
      // Default: persona home directory, with logs/ prefix in category
      this.logRootIsLogsDir = false;
      this.logRoot = path.join(SystemPaths.root, 'personas', uniqueId);
      this.logger = Logger.create(componentName, `logs/${subsystem}`, this.logRoot);
    }
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
    const baseName = fileName.replace(/\.log$/, '');

    // Check if logging is enabled for this persona + category
    if (!LoggingConfig.isEnabled(this.uniqueId, baseName)) {
      return; // Early exit - logging disabled for this persona/category
    }

    const componentName = `${this.uniqueId}:${baseName}`;

    // If logRoot is already the logs directory, don't add logs/ prefix
    const category = this.logRootIsLogsDir ? baseName : `logs/${baseName}`;

    // Create a ComponentLogger for this file using the same logRoot as this logger
    const fileLogger = Logger.create(componentName, category, this.logRoot);
    fileLogger.writeRaw(message + '\n');
  }
}
