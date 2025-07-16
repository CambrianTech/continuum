// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * Health Module - Middle-out Architecture Export
 * 
 * Self-contained module for system health monitoring functionality
 * following the middle-out architecture pattern
 */

// Main command exports
export { HealthCommand } from './HealthCommand';
export { default as HealthCommandDefault } from './HealthCommand';

// Types and interfaces
export type { 
  HealthStatus, 
  HealthMetrics, 
  ConsoleTestResults, 
  HealthReport 
} from './HealthCommand';

// Future: Client/shared exports when we add them
// export { HealthClient } from './client/HealthClient';
// export { HealthFormatter } from './shared/HealthFormatter';