/**
 * JTAG Router Types and Configuration
 * 
 * Centralized type definitions and default configuration for JTAGRouter.
 * Allows configuration to be passed down from system level.
 */

/**
 * Queue configuration for JTAGRouter
 */
export interface JTAGRouterQueueConfig {
  readonly enableDeduplication: boolean;
  readonly deduplicationWindow: number;
  readonly maxSize: number;
  readonly maxRetries: number;
  readonly flushInterval: number;
}

/**
 * Health monitoring configuration for JTAGRouter
 */
export interface JTAGRouterHealthConfig {
  readonly enableHealthMonitoring: boolean;
  readonly healthCheckInterval: number;
  readonly connectionTimeout: number;
}

/**
 * Response correlation configuration for JTAGRouter
 */
export interface JTAGRouterResponseConfig {
  readonly correlationTimeout: number;
  readonly enableCorrelation: boolean;
}

/**
 * Complete configuration interface for JTAGRouter
 */
export interface JTAGRouterConfig {
  readonly queue?: Partial<JTAGRouterQueueConfig>;
  readonly health?: Partial<JTAGRouterHealthConfig>;
  readonly response?: Partial<JTAGRouterResponseConfig>;
  readonly enableLogging?: boolean;
}

/**
 * Resolved configuration with all required fields
 */
export interface ResolvedJTAGRouterConfig {
  readonly queue: JTAGRouterQueueConfig;
  readonly health: JTAGRouterHealthConfig;
  readonly response: JTAGRouterResponseConfig;
  readonly enableLogging: boolean;
}

/**
 * Default JTAGRouter configuration
 * Can be used as a base and overridden at system level
 */
export const DEFAULT_JTAG_ROUTER_CONFIG: ResolvedJTAGRouterConfig = {
  queue: {
    enableDeduplication: true,
    deduplicationWindow: 60000, // 1 minute for console error deduplication
    maxSize: 1000,
    maxRetries: 3,
    flushInterval: 500
  },
  health: {
    enableHealthMonitoring: true,
    healthCheckInterval: 30000, // 30 seconds
    connectionTimeout: 10000 // 10 seconds
  },
  response: {
    correlationTimeout: 30000, // 30 second timeout for commands
    enableCorrelation: true
  },
  enableLogging: true
} as const;

/**
 * Create resolved JTAGRouter config by merging user config with defaults
 */
export function createJTAGRouterConfig(config: JTAGRouterConfig = {}): ResolvedJTAGRouterConfig {
  return {
    queue: {
      ...DEFAULT_JTAG_ROUTER_CONFIG.queue,
      ...config.queue
    },
    health: {
      ...DEFAULT_JTAG_ROUTER_CONFIG.health,
      ...config.health
    },
    response: {
      ...DEFAULT_JTAG_ROUTER_CONFIG.response,
      ...config.response
    },
    enableLogging: config.enableLogging ?? DEFAULT_JTAG_ROUTER_CONFIG.enableLogging
  } as const;
}