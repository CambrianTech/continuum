// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * Info Module - Middle-out Architecture Export
 * 
 * Self-contained module for system information and status display
 * following the middle-out architecture pattern
 */

// Main command exports
export { InfoCommand } from './InfoCommand';

// Types and interfaces
export type { 
  SystemInfo, 
  InfoResult, 
  ContinuumInfo, 
  DaemonInfo, 
  InfoParams 
} from './types';

// Future: Client/shared exports when we add them
// export { InfoClient } from './client/InfoClient';
// export { InfoFormatter } from './shared/InfoFormatter';