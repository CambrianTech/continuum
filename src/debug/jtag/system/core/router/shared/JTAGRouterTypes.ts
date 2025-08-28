/**
 * JTAG Router Types and Configuration
 * 
 * Centralized type definitions and default configuration for JTAGRouter.
 * Allows configuration to be passed down from system level.
 */

import type { TransportRole, TransportEndpointStatus } from '../../../transports';
import { TRANSPORT_ROLES } from '../../../transports';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { JTAGEnvironment } from '../../types/JTAGTypes';
import { SYSTEM_SCOPES } from '../../types/SystemScopes';
import type { JTAGMessageQueue } from './queuing/JTAGMessageQueue';
import type { ConnectionHealthManager } from './ConnectionHealthManager';

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
 * Transport configuration for JTAGRouter
 */
export interface JTAGRouterTransportConfig {
  readonly preferred: 'websocket' | 'http' | 'udp-multicast';
  readonly fallback: boolean;
  readonly role: TransportRole;
  readonly serverPort?: number;
  readonly serverUrl?: string;
  readonly enableP2P?: boolean;
  readonly strategy?: 'hardcoded' | 'dynamic';
  readonly forceLegacy?: boolean; // Explicit opt-in to legacy behavior
}

/**
 * Complete configuration interface for JTAGRouter
 */
export interface JTAGRouterConfig {
  readonly queue?: Partial<JTAGRouterQueueConfig>;
  readonly health?: Partial<JTAGRouterHealthConfig>;
  readonly response?: Partial<JTAGRouterResponseConfig>;
  readonly transport?: Partial<JTAGRouterTransportConfig>;
  readonly enableLogging?: boolean;
  readonly sessionId: UUID; // Required session ID for transport handshake
}

/**
 * Resolved configuration with all required fields
 */
export interface ResolvedJTAGRouterConfig {
  readonly queue: JTAGRouterQueueConfig;
  readonly health: JTAGRouterHealthConfig;
  readonly response: JTAGRouterResponseConfig;
  readonly transport: JTAGRouterTransportConfig;
  readonly enableLogging: boolean;
  readonly sessionId: UUID; // Required session ID for transport handshake
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
    correlationTimeout: 60000, // 60 second timeout for commands (allows for full system startup)
    enableCorrelation: true
  },
  transport: {
    preferred: 'websocket',
    fallback: true,
    role: TRANSPORT_ROLES.SERVER, // Default to server role (browsers will override to 'client')
    serverPort: 9001 as const, // Default - actual port from JTAGContext.config.instance.ports.websocket_server
    serverUrl: undefined // Will be auto-derived from port
  },
  enableLogging: true,
  sessionId: SYSTEM_SCOPES.SYSTEM // System default session ID
} as const;

/**
 * Create resolved JTAGRouter config by merging user config with defaults
 */
export function createJTAGRouterConfig(config: JTAGRouterConfig): ResolvedJTAGRouterConfig {
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
    transport: {
      ...DEFAULT_JTAG_ROUTER_CONFIG.transport,
      ...config.transport
    },
    enableLogging: config.enableLogging ?? DEFAULT_JTAG_ROUTER_CONFIG.enableLogging,
    sessionId: config.sessionId
  } as const;
}

/**
 * Router Status Interface - Strongly typed router status information
 * 
 * Provides comprehensive view of router state with compile-time safety.
 * Eliminates any usage by leveraging TypeScript's ReturnType utility.
 */
export interface RouterStatus {
  /** Current router environment */
  readonly environment: JTAGEnvironment;
  /** Whether router has been initialized */
  readonly initialized: boolean;
  /** Number of registered message subscribers */
  readonly subscribers: number;
  /** Primary transport connection status */
  readonly transport: {
    readonly name: string;
    readonly connected: boolean;
  } | null;
  /** Message queue status from JTAGMessageQueue */
  readonly queue: ReturnType<JTAGMessageQueue['getStatus']>;
  /** Connection health status from ConnectionHealthManager */
  readonly health: ReturnType<ConnectionHealthManager['getHealth']>;
  /** Detailed transport status for all transports */
  readonly transportStatus: TransportEndpointStatus;
}

// Transport status types now use TransportEndpointStatus from transport system
// This eliminates duplication and ensures consistency with transport interfaces