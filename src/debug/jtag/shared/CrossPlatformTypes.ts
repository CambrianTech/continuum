// ISSUES: 0 open, last updated 2025-07-23 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * ✅ RESOLVED:
 * - [x] Issue #1: getPerformance() detection broken - performance is available in both environments (FIXED: direct performance.now() usage)
 * - [x] Issue #2: Remove unused getProcess() and getPerformance() functions (FIXED: no unused functions found)
 */

/**
 * Cross-Platform Type Definitions
 * 
 * Defines types that work correctly in both browser and Node.js environments.
 * Provides consistent APIs for timing, process info, and other platform-specific features.
 * 
 * CORE ARCHITECTURE:
 * - TimerHandle: Unified timer type for setTimeout/setInterval
 * - ProcessLike: Optional process information interface
 * - PerformanceLike: Consistent high-resolution timing interface
 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: Type compatibility across environments
 * - Integration tests: Actual browser vs Node.js behavior
 * - Performance tests: Timer precision and overhead
 * 
 * ARCHITECTURAL INSIGHTS:
 * - ReturnType pattern ensures timer compatibility automatically
 * - Optional interfaces allow graceful feature detection
 * - Performance API is actually available in both modern environments
 * - Process detection enables environment-specific optimizations
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

// Cross-platform performance timing
// Note: performance.now() is available in both modern browsers and Node.js
export const getHighResolutionTime = (): number => {
  // Both environments have performance.now() - just use it directly
  return performance.now();
};

// Cross-platform process information
export const getProcessInfo = (): ProcessLike => {
  if (typeof process !== 'undefined' && typeof process.uptime === 'function') {
    return process; // Node.js
  }
  // Browser fallback - no process info available
  return {};
};