// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * FileSaveCommand - Direct export for command discovery
 * 
 * This file exists to enable the UniversalCommandRegistry to discover the command.
 * It re-exports the actual command implementation from the server directory.
 */

export { FileSaveCommand } from './server/FileSaveCommand';