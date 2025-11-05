/**
 * RouterConstants - Centralized constants for router system
 * 
 * All magic strings and configuration values used throughout the router system.
 * This enables strong typing and prevents typos in string literals.
 */

export const ROUTER_CONSTANTS = {
  // Router Names
  UNIVERSAL_ROUTER: 'universal-router' as const,
  
  // Special Endpoints
  CLIENT_ENDPOINT: 'client' as const,
  
  // Transport Strategy Types
  STRATEGY_HARDCODED: 'hardcoded' as const,
  STRATEGY_DYNAMIC: 'dynamic' as const,
  
  // Environment Variables
  ENV_JTAG_FORCE_LEGACY: 'JTAG_FORCE_LEGACY' as const,
  
  // Processing Token Prefixes
  REQUEST_TOKEN_PREFIX: 'req:' as const,
  RESPONSE_TOKEN_PREFIX: 'res:' as const,
  CLIENT_CORRELATION_PREFIX: 'client_' as const,
  
  // Environment Types
  ENVIRONMENT_BROWSER: 'browser' as const,
  ENVIRONMENT_SERVER: 'server' as const, 
  ENVIRONMENT_REMOTE: 'remote' as const,
  
  // Environment Prefixes (for endpoint parsing)
  ENV_PREFIX_BROWSER: 'browser/' as const,
  ENV_PREFIX_SERVER: 'server/' as const,
  ENV_PREFIX_REMOTE: 'remote/' as const,
  
  // Transport Role Types
  ROLE_SERVER: 'server' as const,
  ROLE_CLIENT: 'client' as const,
  ROLE_BROWSER: 'browser' as const,
  
  // Transport Protocol Types
  TRANSPORT_WEBSOCKET: 'websocket' as const,
  TRANSPORT_HTTP: 'http' as const,
  
  // Message Routing Patterns
  REMOTE_ENDPOINT_PREFIX: '/remote/' as const,
  HIERARCHICAL_MATCH_TYPE: 'hierarchical' as const,
  EXACT_MATCH_TYPE: 'exact' as const,
  
  // Health and Connection States
  HEALTH_HEALTHY: 'healthy' as const,
  HEALTH_UNHEALTHY: 'unhealthy' as const,
  HEALTH_CONNECTING: 'connecting' as const,
  HEALTH_DISCONNECTED: 'disconnected' as const,
  
  // Common Command Types (for priority determination)
  COMMAND_TYPES: {
    CHAT: 'chat' as const,
    FILE: 'file' as const,
    SCREENSHOT: 'screenshot' as const,
    CONSOLE: 'console' as const,
    HEALTH: 'health' as const,
    LIST: 'list' as const
  },
  
  // Timeout Values (milliseconds)
  TIMEOUTS: {
    MESSAGE_PROCESSING: 30000,
    CORRELATION: 60000,
    TRANSPORT_CONNECT: 10000
  } as const
} as const;

// Type definitions for better type safety
export type RouterName = typeof ROUTER_CONSTANTS.UNIVERSAL_ROUTER;
export type SpecialEndpoint = typeof ROUTER_CONSTANTS.CLIENT_ENDPOINT;
export type TransportStrategy = typeof ROUTER_CONSTANTS.STRATEGY_HARDCODED | typeof ROUTER_CONSTANTS.STRATEGY_DYNAMIC;
export type TransportRole = typeof ROUTER_CONSTANTS.ROLE_SERVER | typeof ROUTER_CONSTANTS.ROLE_CLIENT | typeof ROUTER_CONSTANTS.ROLE_BROWSER;
export type MatchType = typeof ROUTER_CONSTANTS.HIERARCHICAL_MATCH_TYPE | typeof ROUTER_CONSTANTS.EXACT_MATCH_TYPE;
export type HealthState = typeof ROUTER_CONSTANTS.HEALTH_HEALTHY | typeof ROUTER_CONSTANTS.HEALTH_UNHEALTHY | typeof ROUTER_CONSTANTS.HEALTH_CONNECTING | typeof ROUTER_CONSTANTS.HEALTH_DISCONNECTED;
export type CommandType = typeof ROUTER_CONSTANTS.COMMAND_TYPES[keyof typeof ROUTER_CONSTANTS.COMMAND_TYPES];

// Re-export for convenience
export const {
  UNIVERSAL_ROUTER,
  CLIENT_ENDPOINT,
  STRATEGY_HARDCODED,
  STRATEGY_DYNAMIC,
  ENV_JTAG_FORCE_LEGACY,
  REQUEST_TOKEN_PREFIX,
  RESPONSE_TOKEN_PREFIX,
  CLIENT_CORRELATION_PREFIX,
  ENVIRONMENT_BROWSER,
  ENVIRONMENT_SERVER,
  ENVIRONMENT_REMOTE,
  ENV_PREFIX_BROWSER,
  ENV_PREFIX_SERVER,
  ENV_PREFIX_REMOTE,
  ROLE_SERVER,
  ROLE_CLIENT,
  ROLE_BROWSER,
  TRANSPORT_WEBSOCKET,
  TRANSPORT_HTTP,
  REMOTE_ENDPOINT_PREFIX,
  HIERARCHICAL_MATCH_TYPE,
  EXACT_MATCH_TYPE,
  COMMAND_TYPES,
  TIMEOUTS
} = ROUTER_CONSTANTS;