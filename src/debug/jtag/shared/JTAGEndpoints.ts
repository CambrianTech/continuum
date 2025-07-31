/**
 * JTAG Endpoint Constants - Centralized, Type-Safe Endpoint Management
 * 
 * Single source of truth for all JTAG system endpoints to prevent typos
 * and ensure consistency across browser/server contexts.
 */

// Core daemon endpoints (without context prefix)
export const JTAG_DAEMON_ENDPOINTS = {
  HEALTH: 'health',
  CONSOLE: 'console', 
  COMMANDS: 'commands',
  EVENTS: 'events',
  SESSION: 'session-daemon'
} as const;

// Full endpoint paths (with context prefixes)
export const JTAG_ENDPOINTS = {
  // Health endpoints (daemon-based)
  HEALTH: {
    BASE: 'health',
    PING: 'health/ping',
    STATUS: 'health/status', 
    METRICS: 'health/metrics'
  },

  // Console endpoints (context-specific)
  CONSOLE: {
    BASE: 'console',
    SERVER: 'server/console',
    BROWSER: 'browser/console'
  },

  // Command endpoints (context-specific)  
  COMMANDS: {
    BASE: 'commands',
    SERVER: 'server/commands',
    BROWSER: 'browser/commands'
  },

  // Event system endpoints
  EVENTS: {
    BASE: 'events',
    SYSTEM: 'system/events',
    CUSTOM: 'custom/events'
  },

  // Session daemon endpoints
  SESSION: {
    BASE: 'session-daemon',
    SERVER: 'server/session-daemon',
    BROWSER: 'browser/session-daemon',
    // Session-specific operations
    GET_DEFAULT: 'session-daemon/get-default',
    GET_CURRENT: 'session-daemon/current',
    CREATE: 'session-daemon/create',
    LIST: 'session-daemon/list',
    ACTIVATE: 'session-daemon/activate',
    DEACTIVATE: 'session-daemon/deactivate',
    END: 'session-daemon/end'
  },

  // HTTP API endpoints
  HTTP: {
    HEALTH: '/health',
    STATUS: '/status',
    JTAG_HEALTH: '/jtag/health'
  }
} as const;

// Strong TypeScript types
export type JTAGContext = 'server' | 'browser';
export type JTAGDaemonEndpoint = typeof JTAG_DAEMON_ENDPOINTS[keyof typeof JTAG_DAEMON_ENDPOINTS];

// Extract all possible endpoint values into a union type
type ExtractValues<T> = T extends Record<string, infer U> 
  ? U extends Record<string, infer V> 
    ? V 
    : U extends string 
      ? U 
      : never
  : never;

export type JTAGEndpointType = ExtractValues<typeof JTAG_ENDPOINTS>;

// Context-specific endpoint types
export type ConsoleEndpoint = typeof JTAG_ENDPOINTS.CONSOLE[keyof typeof JTAG_ENDPOINTS.CONSOLE];
export type CommandEndpoint = typeof JTAG_ENDPOINTS.COMMANDS[keyof typeof JTAG_ENDPOINTS.COMMANDS];
export type HealthEndpoint = typeof JTAG_ENDPOINTS.HEALTH[keyof typeof JTAG_ENDPOINTS.HEALTH];

// Validation helpers
export function isValidEndpoint(endpoint: string): endpoint is JTAGEndpointType {
  const validEndpoints: string[] = [];
  
  Object.values(JTAG_ENDPOINTS).forEach(category => {
    if (typeof category === 'string') {
      validEndpoints.push(category);
    } else {
      validEndpoints.push(...Object.values(category));
    }
  });
  
  return validEndpoints.includes(endpoint);
}