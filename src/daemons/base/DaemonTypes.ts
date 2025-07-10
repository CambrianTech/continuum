/**
 * Strongly-typed daemon identifiers to prevent runtime errors
 * These must match the daemon's registered name exactly
 */

export enum DaemonType {
  // Core system daemons
  SESSION_MANAGER = 'session-manager',
  WEBSOCKET_SERVER = 'websocket-server',
  RENDERER = 'renderer',
  COMMAND_PROCESSOR = 'command-processor',
  BROWSER_MANAGER = 'browser-manager',
  DAEMON_MANAGER = 'daemon-manager',
  MESH_COORDINATOR = 'mesh-coordinator',
  
  // Service daemons
  CONTINUUM_DIRECTORY = 'continuum-directory',
  STATIC_FILE = 'static-file',
  CHATROOM = 'chatroom',
  ACADEMY = 'academy',
  DATABASE = 'database',
  WIDGET = 'widget',
  
  // Mesh and persona daemons
  MESH = 'mesh',
  PERSONA = 'persona',
  
  // Integration daemons (future)
  PORTAL = 'portal',
  GIT_HOOK = 'git-hook',
  IDE = 'ide',
  SLACK = 'slack',
  DISCORD = 'discord',
  GITHUB = 'github'
}

/**
 * Type guard to check if a string is a valid daemon type
 */
export function isValidDaemonType(type: string): type is DaemonType {
  return Object.values(DaemonType).includes(type as DaemonType);
}

/**
 * Get all registered daemon types
 */
export function getAllDaemonTypes(): DaemonType[] {
  return Object.values(DaemonType);
}

/**
 * Daemon categories for organizational purposes
 */
export enum DaemonCategory {
  CORE = 'core',
  SERVICE = 'service',
  INTEGRATION = 'integration'
}

/**
 * Map daemons to their categories
 */
export const DAEMON_CATEGORIES: Record<DaemonType, DaemonCategory> = {
  [DaemonType.SESSION_MANAGER]: DaemonCategory.CORE,
  [DaemonType.WEBSOCKET_SERVER]: DaemonCategory.CORE,
  [DaemonType.RENDERER]: DaemonCategory.CORE,
  [DaemonType.COMMAND_PROCESSOR]: DaemonCategory.CORE,
  [DaemonType.BROWSER_MANAGER]: DaemonCategory.CORE,
  [DaemonType.DAEMON_MANAGER]: DaemonCategory.CORE,
  [DaemonType.MESH_COORDINATOR]: DaemonCategory.CORE,
  
  [DaemonType.CONTINUUM_DIRECTORY]: DaemonCategory.SERVICE,
  [DaemonType.STATIC_FILE]: DaemonCategory.SERVICE,
  [DaemonType.CHATROOM]: DaemonCategory.SERVICE,
  [DaemonType.ACADEMY]: DaemonCategory.SERVICE,
  [DaemonType.DATABASE]: DaemonCategory.SERVICE,
  [DaemonType.WIDGET]: DaemonCategory.SERVICE,
  
  [DaemonType.MESH]: DaemonCategory.INTEGRATION,
  [DaemonType.PERSONA]: DaemonCategory.INTEGRATION,
  [DaemonType.PORTAL]: DaemonCategory.INTEGRATION,
  [DaemonType.GIT_HOOK]: DaemonCategory.INTEGRATION,
  [DaemonType.IDE]: DaemonCategory.INTEGRATION,
  [DaemonType.SLACK]: DaemonCategory.INTEGRATION,
  [DaemonType.DISCORD]: DaemonCategory.INTEGRATION,
  [DaemonType.GITHUB]: DaemonCategory.INTEGRATION
};