/**
 * Training Log Entity
 *
 * Tracks detailed execution logs during training (stdout, stderr, warnings, errors).
 * Multiple log entries belong to one TrainingSessionEntity, providing debugging
 * and troubleshooting data.
 *
 * Integration:
 * - Created automatically during training for all log output
 * - Queried by training/status for recent logs
 * - Used for debugging training failures
 * - Can be filtered by log level and searched
 *
 * Purpose:
 * - Debugging training failures and issues
 * - Audit trail of training execution
 * - Capture provider-specific warnings/errors
 * - Historical analysis of training runs
 */

import { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import {
  TextField,
  JsonField,
  NumberField,
  EnumField
} from '../../../../system/data/decorators/FieldDecorators';

import { DataList } from '../../../../commands/data/list/shared/DataListTypes';
/**
 * Log level enumeration
 */
export type LogLevel = 'debug' | 'info' | 'warning' | 'error';

/**
 * Log source enumeration
 */
export type LogSource = 'stdout' | 'stderr' | 'system' | 'provider';

/**
 * Training Log Entity
 *
 * Individual log message captured during training.
 * Stored frequently for comprehensive debugging capability.
 *
 * @example
 * ```typescript
 * // Create log entry during training
 * const log = TrainingLogEntity.create({
 *   sessionId: session.id,
 *   level: 'info',
 *   source: 'stdout',
 *   message: 'Epoch 1/3 complete - loss: 0.42'
 * });
 *
 * // Query error logs
 * const errors = await DataList.execute({
 *   collection: 'training_logs',
 *   filter: {
 *     sessionId: session.id,
 *     level: 'error'
 *   },
 *   orderBy: [{ field: 'timestamp', direction: 'desc' }],
 *   limit: 50
 * });
 *
 * // Search logs
 * const cudaLogs = await DataList.execute({
 *   collection: 'training_logs',
 *   filter: {
 *     sessionId: session.id,
 *     message: { $regex: 'CUDA', $options: 'i' }
 *   }
 * });
 * ```
 */
export class TrainingLogEntity extends BaseEntity {
  /**
   * Collection name for data storage
   */
  static readonly collection = 'training_logs';

  /**
   * Get collection name (required by ORM)
   */
  get collection(): string {
    return TrainingLogEntity.collection;
  }

  /**
   * Training session this log entry belongs to
   */
  @TextField()
  sessionId!: UUID;

  /**
   * Log level (debug, info, warning, error)
   */
  @EnumField()
  level!: LogLevel;

  /**
   * Log source (stdout, stderr, system, provider)
   */
  @EnumField()
  source!: LogSource;

  /**
   * Log message content
   */
  @TextField()
  message!: string;

  /**
   * Training step when log was generated (null if not during training loop)
   */
  @NumberField({ nullable: true })
  step!: number | null;

  /**
   * Epoch when log was generated (null if not during training loop)
   */
  @NumberField({ nullable: true })
  epoch!: number | null;

  /**
   * Stack trace (populated for error level logs)
   */
  @TextField({ nullable: true })
  stackTrace!: string | null;

  /**
   * Metadata for extensibility (additional context, provider-specific data)
   */
  @JsonField()
  metadata!: Record<string, unknown>;

  /**
   * Validate entity data
   */
  validate(): { success: boolean; error?: string } {
    // Required fields
    if (!this.sessionId) {
      return { success: false, error: 'sessionId is required' };
    }
    if (!this.level) {
      return { success: false, error: 'level is required' };
    }
    if (!this.source) {
      return { success: false, error: 'source is required' };
    }
    if (!this.message) {
      return { success: false, error: 'message is required' };
    }

    // Validate enum values
    const validLevels: LogLevel[] = ['debug', 'info', 'warning', 'error'];
    if (!validLevels.includes(this.level)) {
      return { success: false, error: `Invalid log level: ${this.level}` };
    }

    const validSources: LogSource[] = ['stdout', 'stderr', 'system', 'provider'];
    if (!validSources.includes(this.source)) {
      return { success: false, error: `Invalid log source: ${this.source}` };
    }

    // Validate optional numeric fields
    if (this.step !== null && (typeof this.step !== 'number' || this.step < 0)) {
      return { success: false, error: 'step must be non-negative number or null' };
    }
    if (this.epoch !== null && (typeof this.epoch !== 'number' || this.epoch < 0)) {
      return { success: false, error: 'epoch must be non-negative number or null' };
    }

    // Initialize metadata if not set
    if (!this.metadata) {
      this.metadata = {};
    }

    return { success: true };
  }

  /**
   * Create log from Error object (static helper)
   *
   * @param sessionId - Training session ID
   * @param error - Error object
   * @param step - Optional training step
   * @param epoch - Optional epoch
   * @returns Creation result with entity or error
   */
  static createFromError(
    sessionId: UUID,
    error: Error,
    step?: number,
    epoch?: number
  ): { success: boolean; entity?: TrainingLogEntity; error?: string } {
    return TrainingLogEntity.create({
      sessionId,
      level: 'error',
      source: 'system',
      message: error.message,
      step: step ?? null,
      epoch: epoch ?? null,
      stackTrace: error.stack ?? null,
      metadata: {
        errorName: error.name,
        errorConstructor: error.constructor.name
      }
    });
  }

  /**
   * Check if this is an error log
   */
  isError(): boolean {
    return this.level === 'error';
  }

  /**
   * Check if this is a warning log
   */
  isWarning(): boolean {
    return this.level === 'warning';
  }

  /**
   * Get formatted log message with metadata
   *
   * @returns Formatted string for display
   */
  getFormattedMessage(): string {
    const timestamp = new Date(this.createdAt).toISOString();
    const levelPrefix = `[${this.level.toUpperCase()}]`;
    const sourcePrefix = `[${this.source}]`;
    const stepPrefix = this.step !== null ? `[step ${this.step}]` : '';

    let msg = `${timestamp} ${levelPrefix}${sourcePrefix}${stepPrefix} ${this.message}`;

    if (this.stackTrace) {
      msg += `\n${this.stackTrace}`;
    }

    return msg;
  }

  /**
   * Get short summary for UI display (truncated message)
   *
   * @param maxLength - Maximum message length (default: 100)
   * @returns Truncated message
   */
  getShortSummary(maxLength: number = 100): string {
    if (this.message.length <= maxLength) {
      return this.message;
    }
    return this.message.substring(0, maxLength - 3) + '...';
  }

  /**
   * Check if log message matches pattern
   *
   * @param pattern - Regex pattern or substring to match
   * @returns True if message matches
   */
  matches(pattern: string | RegExp): boolean {
    if (typeof pattern === 'string') {
      return this.message.toLowerCase().includes(pattern.toLowerCase());
    }
    return pattern.test(this.message);
  }

  /**
   * Get color code for log level (for terminal output)
   *
   * @returns ANSI color code
   */
  getColorCode(): string {
    switch (this.level) {
      case 'debug':
        return '\x1b[90m'; // Gray
      case 'info':
        return '\x1b[36m'; // Cyan
      case 'warning':
        return '\x1b[33m'; // Yellow
      case 'error':
        return '\x1b[31m'; // Red
      default:
        return '\x1b[0m'; // Reset
    }
  }

  /**
   * Get log severity score (for sorting/filtering)
   *
   * @returns Numeric score (higher = more severe)
   */
  getSeverityScore(): number {
    switch (this.level) {
      case 'debug':
        return 0;
      case 'info':
        return 1;
      case 'warning':
        return 2;
      case 'error':
        return 3;
      default:
        return 0;
    }
  }

  /**
   * Get colored formatted message for terminal
   *
   * @returns Message with ANSI color codes
   */
  getColoredMessage(): string {
    const colorCode = this.getColorCode();
    const resetCode = '\x1b[0m';
    return `${colorCode}${this.getFormattedMessage()}${resetCode}`;
  }
}
