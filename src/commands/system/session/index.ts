/**
 * Session Commands Module
 * 
 * Complete command interface for session management
 * All commands interact with SessionManagerDaemon via structured messages
 */

export { SessionCommand } from './SessionCommand';
export { SessionListCommand } from './SessionListCommand';
export { SessionInfoCommand } from './SessionInfoCommand';
export { SessionCreateCommand } from './SessionCreateCommand';
export { SessionConnectCommand } from './SessionConnectCommand';
export { SessionPathsCommand } from './SessionPathsCommand';
export { SessionStatsCommand } from './SessionStatsCommand';

/**
 * Command Usage Examples:
 * 
 * // List all sessions
 * session-list
 * session-list --filter='{"active": true}'
 * session-list --format=table
 * 
 * // Get session info
 * session-info --sessionId=cli-joel-dev-250701-1234
 * session-info --sessionId=cli-joel-dev-250701-1234 --includeArtifacts=true
 * 
 * // Create new session
 * session-create --starter=cli --name=joel --type=development --project=continuum
 * session-create --starter=portal --name=debug-session --type=debugging --branch=feature-widgets
 * 
 * // Connect to existing or create new (CLI default behavior)
 * session-connect --name=joel --starter=cli --project=continuum
 * session-connect --name=joel --forceNew=true
 * 
 * // Get session paths for integration
 * session-paths --sessionId=cli-joel-dev-250701-1234
 * session-paths --owner=joel --pathType=logs
 * session-paths --owner=joel --format=shell
 * 
 * // Get system statistics
 * session-stats
 * session-stats --includeDetails=true --groupBy=type
 * 
 * // Legacy unified command (still supported)
 * session --action=list
 * session --action=info --sessionId=cli-joel-dev-250701-1234
 */

export const SESSION_COMMANDS = [
  'session',
  'session-list',
  'session-info', 
  'session-create',
  'session-connect',
  'session-paths',
  'session-stats'
] as const;

export type SessionCommandName = typeof SESSION_COMMANDS[number];