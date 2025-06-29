/**
 * Error handling utilities - Clean, typed error message extraction
 * Eliminates the need for duplicated error handling patterns
 */

/**
 * Extract error message safely from unknown error type
 */
export function getErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  
  if (typeof error === 'string') {
    return error;
  }
  
  if (error && typeof error === 'object' && 'message' in error) {
    return String((error as { message: unknown }).message);
  }
  
  return String(error);
}

/**
 * Extract error name safely from unknown error type
 */
export function getErrorName(error: unknown): string {
  if (error instanceof Error && error.name) {
    return error.name;
  }
  
  return 'UnknownError';
}

/**
 * Create standardized error info object
 */
export interface ErrorInfo {
  name: string;
  message: string;
  stack?: string;
}

export function getErrorInfo(error: unknown): ErrorInfo {
  return {
    name: getErrorName(error),
    message: getErrorMessage(error),
    stack: error instanceof Error ? error.stack : undefined
  };
}