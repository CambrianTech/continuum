/**
 * SubsystemLogger - Base class for segregated logging in Being Architecture
 *
 * Provides file-based logging per persona and subsystem (Mind/Body/Soul/CNS).
 * Reduces console.log spam by routing logs to appropriate files.
 *
 * Directory structure:
 * .continuum/logs/personas/{personaId}/
 *   ├── mind.log       (cognition, state tracking)
 *   ├── body.log       (action, execution, tools)
 *   ├── soul.log       (memory, learning, genome)
 *   └── cns.log        (orchestration, coordination)
 *
 * Usage:
 * ```typescript
 * class PersonaMind {
 *   private logger: SubsystemLogger;
 *
 *   constructor(personaUser: PersonaUserForMind) {
 *     this.logger = new SubsystemLogger('mind', personaUser.id, personaUser.displayName);
 *     this.logger.info('Mind initialized');
 *   }
 * }
 * ```
 */

import * as fs from 'fs';
import * as path from 'path';
import type { UUID } from '../../../../../core/types/CrossPlatformUUID';
import { SystemPaths } from '../../../../../core/config/SystemPaths';

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';
export type Subsystem = 'mind' | 'body' | 'soul' | 'cns';

export interface LoggerConfig {
  /** Enable console output (default: true for errors/warnings, false for info/debug) */
  enableConsole?: boolean;
  /** Enable file output (default: true) */
  enableFile?: boolean;
  /** Minimum log level (default: 'debug') */
  minLevel?: LogLevel;
  /** Custom log directory (default: .continuum/logs/personas/{personaId}) */
  logDir?: string;
}

const LOG_LEVELS: Record<LogLevel, number> = {
  debug: 0,
  info: 1,
  warn: 2,
  error: 3
};

export class SubsystemLogger {
  private readonly subsystem: Subsystem;
  private readonly personaId: UUID;
  private readonly personaName: string;
  private readonly config: Required<LoggerConfig>;
  private readonly logFilePath: string;
  private writeStream: fs.WriteStream | null = null;

  constructor(
    subsystem: Subsystem,
    personaId: UUID,
    personaName: string,
    config: LoggerConfig = {}
  ) {
    this.subsystem = subsystem;
    this.personaId = personaId;
    this.personaName = personaName;

    // Default config
    this.config = {
      enableConsole: config.enableConsole ?? false, // Default to file-only logging
      enableFile: config.enableFile ?? true,
      minLevel: config.minLevel ?? 'debug',
      logDir: config.logDir ?? this.getDefaultLogDir()
    };

    // Construct log file path
    this.logFilePath = path.join(this.config.logDir, `${subsystem}.log`);

    // Initialize log directory and file
    if (this.config.enableFile) {
      this.initializeLogFile();
    }
  }

  /**
   * Get default log directory based on persona name (not UUID)
   * Uses SystemPaths - SINGLE SOURCE OF TRUTH for all paths
   */
  private getDefaultLogDir(): string {
    return SystemPaths.logs.personas(this.personaName);
  }

  /**
   * Initialize log directory and file stream
   */
  private initializeLogFile(): void {
    try {
      // Create log directory if it doesn't exist
      if (!fs.existsSync(this.config.logDir)) {
        fs.mkdirSync(this.config.logDir, { recursive: true });
      }

      // Create write stream (append mode)
      this.writeStream = fs.createWriteStream(this.logFilePath, { flags: 'a' });

      // Write header on new file
      const header = `\n${'='.repeat(80)}\n[${new Date().toISOString()}] ${this.personaName} - ${this.subsystem.toUpperCase()} LOG STARTED\n${'='.repeat(80)}\n`;
      this.writeStream.write(header);

    } catch (error) {
      console.error(`❌ SubsystemLogger: Failed to initialize log file: ${error}`);
      this.config.enableFile = false; // Disable file logging if init fails
    }
  }

  /**
   * Check if a log level should be output
   */
  private shouldLog(level: LogLevel): boolean {
    return LOG_LEVELS[level] >= LOG_LEVELS[this.config.minLevel];
  }

  /**
   * Format log message with timestamp and context
   */
  private formatMessage(level: LogLevel, message: string, ...args: unknown[]): string {
    const timestamp = new Date().toISOString();
    const emoji = this.getEmojiForSubsystem();
    const levelStr = level.toUpperCase().padEnd(5);

    // Format additional arguments
    const argsStr = args.length > 0 ? ' ' + args.map(arg =>
      typeof arg === 'object' ? JSON.stringify(arg, null, 2) : String(arg)
    ).join(' ') : '';

    return `[${timestamp}] ${emoji} ${levelStr} [${this.subsystem}] ${message}${argsStr}`;
  }

  /**
   * Get emoji for subsystem
   */
  private getEmojiForSubsystem(): string {
    const emojiMap: Record<Subsystem, string> = {
      mind: '🧠',
      body: '🤸',
      soul: '🧬',
      cns: '🔮'
    };
    return emojiMap[this.subsystem] || '📋';
  }

  /**
   * Write log entry
   */
  private log(level: LogLevel, message: string, ...args: unknown[]): void {
    if (!this.shouldLog(level)) {
      return;
    }

    const formattedMessage = this.formatMessage(level, message, ...args);

    // Console output (only for errors and warnings by default)
    if (this.config.enableConsole || level === 'error' || level === 'warn') {
      const consoleMethod = level === 'error' ? console.error :
                           level === 'warn' ? console.warn :
                           console.log;
      consoleMethod(formattedMessage);
    }

    // File output
    if (this.config.enableFile && this.writeStream) {
      this.writeStream.write(formattedMessage + '\n');
    }
  }

  // Public API

  debug(message: string, ...args: unknown[]): void {
    this.log('debug', message, ...args);
  }

  info(message: string, ...args: unknown[]): void {
    this.log('info', message, ...args);
  }

  warn(message: string, ...args: unknown[]): void {
    this.log('warn', message, ...args);
  }

  error(message: string, ...args: unknown[]): void {
    this.log('error', message, ...args);
  }

  /**
   * Close log file stream
   */
  close(): void {
    if (this.writeStream) {
      const footer = `${'='.repeat(80)}\n[${new Date().toISOString()}] ${this.personaName} - ${this.subsystem.toUpperCase()} LOG CLOSED\n${'='.repeat(80)}\n\n`;
      this.writeStream.write(footer);
      this.writeStream.end();
      this.writeStream = null;
    }
  }

  /**
   * Get log file path (for debugging)
   */
  getLogFilePath(): string {
    return this.logFilePath;
  }
}
