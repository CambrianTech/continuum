// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * Chat Module - Middle-out Architecture Export
 * 
 * Self-contained module for chat messaging functionality
 * following the middle-out architecture pattern
 */

// Main command exports
export { ChatCommand } from './ChatCommand';
export { default as ChatCommandDefault } from './ChatCommand';

// Future: Client/shared exports when we add them
// export { ChatClient } from './client/ChatClient';
// export { ChatFormatter } from './shared/ChatFormatter';