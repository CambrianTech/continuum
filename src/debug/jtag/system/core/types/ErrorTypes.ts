/**
 * JTAG Error System - Universal Error Types
 * 
 * Built on core TypeScript Error class for maximum compatibility
 * Provides transport portability via JSON serialization
 * Used by all JTAG commands for consistent error handling
 */

/**
 * Base JTAG Error - Extends native Error with transport capability
 */
export abstract class JTAGError extends Error {
  abstract readonly type: string;
  readonly timestamp: string = new Date().toISOString();
  readonly cause?: unknown;
  
  constructor(message: string, options?: { cause?: unknown }) {
    super(message);
    this.name = this.constructor.name;
    this.cause = options?.cause;
  }
  
  /**
   * Serializable representation for transport across JTAG router
   */
  toJSON(): Record<string, unknown> {
    return {
      type: this.type,
      message: this.message,
      name: this.name,
      timestamp: this.timestamp,
      stack: this.stack,
      cause: this.cause
    };
  }
}

/**
 * Validation Error - Parameter or data validation failures
 */
export class ValidationError extends JTAGError {
  readonly type = 'validation' as const;
  
  constructor(
    public readonly field: string,
    message: string,
    options?: { cause?: unknown }
  ) {
    super(`Validation failed for ${field}: ${message}`, options);
  }
}

/**
 * Persistence Error - File system or storage failures
 */
export class PersistenceError extends JTAGError {
  readonly type = 'persistence' as const;
  
  constructor(
    public readonly path: string,
    public readonly operation: 'read' | 'write' | 'delete' | 'create',
    message: string,
    options?: { cause?: unknown }
  ) {
    super(`${operation} failed at ${path}: ${message}`, options);
  }
}

/**
 * Enhancement Error - Plugin or enhancement system failures
 */
export class EnhancementError extends JTAGError {
  readonly type = 'enhancement' as const;
  
  constructor(
    public readonly plugin: string,
    message: string,
    options?: { cause?: unknown }
  ) {
    super(`Plugin ${plugin} failed: ${message}`, options);
  }
}

/**
 * Network Error - Transport or communication failures
 */
export class NetworkError extends JTAGError {
  readonly type = 'network' as const;
  
  constructor(
    public readonly endpoint: string,
    message: string,
    options?: { cause?: unknown }
  ) {
    super(`Network error at ${endpoint}: ${message}`, options);
  }
}

/**
 * Generic Result type for all JTAG operations
 */
export type JTAGResult<T, E extends JTAGError = JTAGError> = 
  | { readonly success: true; readonly data: T }
  | { readonly success: false; readonly error: E };

/**
 * Error context utilities using core TS types
 */
export type ErrorContext<T extends Record<string, unknown>> = Required<Pick<T, keyof T>>;
export type SerializableError = Pick<JTAGError, 'type' | 'message' | 'timestamp'>;