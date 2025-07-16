// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * Browser Console Module - Middle-out Architecture Export
 * 
 * Self-contained module for browser console interaction functionality
 * following the middle-out architecture pattern
 */

// Main command exports
export { BrowserConsoleCommand } from './BrowserConsoleCommand';

// Types and interfaces
export type { BrowserConsoleOptions } from './BrowserConsoleCommand';

// Future: Client/shared exports when we add them
// export { BrowserConsoleClient } from './client/BrowserConsoleClient';
// export { BrowserConsoleFormatter } from './shared/BrowserConsoleFormatter';