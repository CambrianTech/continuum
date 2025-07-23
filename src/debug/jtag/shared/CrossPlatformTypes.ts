/**
 * Cross-Platform Type Definitions
 * 
 * Defines types that work correctly in both browser and Node.js environments.
 */

// Timer handle type that works in both environments
// Browser: setTimeout returns number
// Node.js: setTimeout returns NodeJS.Timeout  
export type TimerHandle = ReturnType<typeof setTimeout>;

// Process-like interface for cross-platform compatibility
export interface ProcessLike {
  uptime?(): number;
}

// Performance-like interface for cross-platform compatibility  
export interface PerformanceLike {
  now(): number;
}

// Global references that work in both environments
export const getPerformance = (): PerformanceLike => {
  if (typeof performance !== 'undefined') {
    return performance; // Browser
  }
  // Node.js fallback
  return {
    now: () => Date.now()
  };
};

export const getProcess = (): ProcessLike => {
  if (typeof process !== 'undefined') {
    return process; // Node.js
  }
  // Browser fallback
  return {};
};