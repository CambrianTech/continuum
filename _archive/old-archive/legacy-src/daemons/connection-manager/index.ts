/**
 * Connection Manager Daemon - Pure WebSocket connection lifecycle management
 * Entry point for daemon discovery
 */

export { ConnectionManagerDaemon } from './server/ConnectionManagerDaemon';
export { ConnectionManagerClient } from './client/ConnectionManagerClient';
export * from './shared/ConnectionMessageTypes';

// Default export for main daemon class
import { ConnectionManagerDaemon } from './server/ConnectionManagerDaemon';
export default ConnectionManagerDaemon;