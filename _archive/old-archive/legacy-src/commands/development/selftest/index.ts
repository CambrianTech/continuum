// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * SelfTest Module - Middle-out Architecture Export
 * 
 * Self-contained module for system health testing and validation
 * following the middle-out architecture pattern
 */

// Main command exports
export { SelfTestCommand } from './SelfTestCommand';

// Types and interfaces (when we add them)
export type { SelfTestResult, HealthCheckOptions } from './types';

// Future: Client/shared exports when we add them
// export { SelfTestClient } from './client/SelfTestClient';
// export { SelfTestValidator } from './shared/SelfTestValidator';