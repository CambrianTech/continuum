/**
 * Test Error Command Types - Enhanced
 * 
 * Comprehensive error testing command for validating error handling at all levels.
 * Supports environment-specific triggers and multi-level error testing.
 */

import type { JTAGContext, JTAGPayload } from '@shared/JTAGTypes';
import type { UUID } from '@shared/CrossPlatformUUID';

/**
 * Error trigger types for comprehensive testing
 */
export type ErrorTrigger = 
  | 'validation-error'     // Parameter validation failure
  | 'execution-error'      // Runtime execution error
  | 'timeout-error'        // Simulated timeout
  | 'network-error'        // Network/transport error
  | 'permission-error'     // Permission/security error
  | 'environment-error'    // Environment-specific error
  | 'async-error'          // Asynchronous operation error
  | 'json-error'           // JSON parsing error
  | 'custom-error'         // Custom error message
  | 'generic';             // Generic error (legacy support)

/**
 * Error levels for testing different error handling layers
 */
export type ErrorLevel = 
  | 'command'              // Error at command execution level
  | 'daemon'               // Error at daemon message handling level
  | 'transport'            // Error at transport/routing level
  | 'client'               // Error at client connection level
  | 'system';              // Error at system initialization level

/**
 * Enhanced test error command parameters
 */
export interface TestErrorParams extends JTAGPayload {
  /** Type of error to trigger */
  errorType: ErrorTrigger;
  /** Level at which to trigger the error */
  level?: ErrorLevel;
  /** Environment where error should occur (default: current) */
  environment?: 'browser' | 'server' | 'both';
  /** Custom error message */
  errorMessage?: string;
  /** Delay before triggering error (ms) */
  delay?: number;
  /** Should error be recoverable */
  recoverable?: boolean;
  /** Whether to actually throw the error */
  shouldThrow?: boolean;
}

/**
 * Enhanced test error command result
 */
export interface TestErrorResult extends JTAGPayload {
  /** Success status (should be false for error tests) */
  success: boolean;
  /** Type of error that was triggered */
  errorType: ErrorTrigger;
  /** Level where error occurred */
  errorLevel?: ErrorLevel;
  /** Environment where error occurred */
  environment: string;
  /** Error message */
  message: string;
  /** Error stack trace (if available) */
  stackTrace?: string;
  /** Timestamp when error occurred */
  timestamp: string;
  /** Whether error was handled gracefully */
  handled?: boolean;
}

/**
 * Create test error parameters with defaults
 */
export function createTestErrorParams(
  context: JTAGContext,
  sessionId: UUID,
  errorType: ErrorTrigger,
  options: Partial<TestErrorParams> = {}
): TestErrorParams {
  return {
    context,
    sessionId,
    errorType,
    level: options.level || 'command',
    environment: options.environment || (context.environment === 'remote' ? 'server' : context.environment),
    errorMessage: options.errorMessage,
    delay: options.delay || 0,
    recoverable: options.recoverable ?? true,
    shouldThrow: options.shouldThrow ?? true,
    ...options
  };
}

/**
 * Create test error result from params and error
 */
export function createTestErrorResult(
  context: JTAGContext, 
  sessionId: UUID, 
  result: Partial<TestErrorResult>
): TestErrorResult {
  return {
    context,
    sessionId,
    success: result.success ?? false,
    errorType: result.errorType || 'generic',
    errorLevel: result.errorLevel,
    environment: result.environment || context.environment,
    message: result.message || 'Test error triggered',
    stackTrace: result.stackTrace,
    timestamp: new Date().toISOString(),
    handled: result.handled ?? true,
    ...result
  };
}