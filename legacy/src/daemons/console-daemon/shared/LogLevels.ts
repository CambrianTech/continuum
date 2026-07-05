/**
 * Console Log Levels - Shared Type Definition
 * 
 * Centralizes the logging level types used across console daemon,
 * events, and response types to ensure consistency.
 */

export type LogLevel = 'log' | 'info' | 'warn' | 'error' | 'debug';

/**
 * @deprecated Use LogLevel instead - maintaining for backward compatibility
 */
export type Levels = LogLevel;