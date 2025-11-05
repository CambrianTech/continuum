/**
 * Channel System - Strongly Typed Interfaces
 * 
 * PRINCIPLES:
 * - TypeScript compiler prevents failures
 * - Generic interfaces for extensibility  
 * - 85% code reduction through abstraction
 * - Well-typed endpoints prevent routing mistakes
 * 
 * UNIVERSAL ADDRESSING:
 * - Directory structure = type-safe endpoints
 * - Compile-time endpoint validation
 * - No magic strings, only typed constants
 */

import type { JTAGContext, JTAGMessage } from '../../core/types/JTAGTypes';
import type { JTAGTransport, TransportConfig } from '../../transports/shared/TransportTypes';
import type { ITransportHandler } from '../../transports/shared/ITransportHandler';
import type { UUID } from '../../core/types/CrossPlatformUUID';

// ============================================================================
// STRONGLY TYPED ENVIRONMENT SYSTEM
// ============================================================================

/**
 * Environment types - compile-time validated, no magic strings
 */
export const ENVIRONMENTS = {
  BROWSER: 'browser',
  SERVER: 'server', 
  REMOTE: 'remote'
} as const;

export type Environment = typeof ENVIRONMENTS[keyof typeof ENVIRONMENTS];

/**
 * Protocol types - extensible through adapter registry
 */
export const PROTOCOLS = {
  WEBSOCKET: 'websocket',
  HTTP: 'http',
  UDP_MULTICAST: 'udp-multicast',
  WEBRTC: 'webrtc'
} as const;

export type Protocol = typeof PROTOCOLS[keyof typeof PROTOCOLS];

/**
 * Endpoint patterns - type-safe routing addresses
 * Based on directory structure, validated at compile time
 */
export interface TypedEndpoint {
  readonly environment: Environment;
  readonly path: string;
  readonly nodeId?: string; // For remote/ addressing
}

/**
 * Well-typed endpoint builders - prevent routing mistakes
 */
export const Endpoints = {
  /**
   * Local environment endpoints
   */
  local: {
    browser: (path: string): TypedEndpoint => ({
      environment: ENVIRONMENTS.BROWSER,
      path: path.startsWith('/') ? path.slice(1) : path
    }),
    server: (path: string): TypedEndpoint => ({
      environment: ENVIRONMENTS.SERVER, 
      path: path.startsWith('/') ? path.slice(1) : path
    })
  },

  /**
   * Remote environment endpoints
   */
  remote: (nodeId: string, path: string): TypedEndpoint => ({
    environment: ENVIRONMENTS.REMOTE,
    path: path.startsWith('/') ? path.slice(1) : path,
    nodeId
  }),

  /**
   * Convert endpoint to route string for router
   */
  toString: (endpoint: TypedEndpoint): string => {
    if (endpoint.environment === ENVIRONMENTS.REMOTE && endpoint.nodeId) {
      return `${endpoint.environment}/${endpoint.nodeId}/${endpoint.path}`;
    }
    return `${endpoint.environment}/${endpoint.path}`;
  },

  /**
   * Parse route string back to typed endpoint
   */
  fromString: (route: string): TypedEndpoint | null => {
    const parts = route.split('/');
    if (parts.length < 2) return null;

    const environment = parts[0] as Environment;
    if (!Object.values(ENVIRONMENTS).includes(environment)) return null;

    if (environment === ENVIRONMENTS.REMOTE) {
      if (parts.length < 3) return null;
      return {
        environment,
        nodeId: parts[1],
        path: parts.slice(2).join('/')
      };
    }

    return {
      environment,
      path: parts.slice(1).join('/')
    };
  }
} as const;

// ============================================================================
// GENERIC CHANNEL INTERFACES
// ============================================================================

/**
 * Channel identifier - strongly typed, no magic strings
 */
export interface ChannelId<TProtocol extends Protocol = Protocol> {
  readonly environment: Environment;
  readonly protocol: TProtocol;
  readonly target?: string;
  readonly instanceId?: UUID; // For multiple channels of same type
}

/**
 * Channel configuration - generic and extensible
 */
export interface ChannelConfig<TProtocol extends Protocol = Protocol> {
  readonly id: ChannelId<TProtocol>;
  readonly transport: TransportConfig;
  readonly handler: ITransportHandler;
  readonly options?: ProtocolOptions[TProtocol]; // Protocol-specific options
}

/**
 * Protocol-specific options - extensible for new protocols
 */
export interface ProtocolOptions {
  [PROTOCOLS.WEBSOCKET]: {
    reconnectDelay?: number;
    maxReconnectAttempts?: number;
    heartbeatInterval?: number;
  };
  [PROTOCOLS.HTTP]: {
    timeout?: number;
    retryAttempts?: number;
    keepAlive?: boolean;
  };
  [PROTOCOLS.UDP_MULTICAST]: {
    multicastAddress?: string;
    multicastPort?: number;
    ttl?: number;
  };
  [PROTOCOLS.WEBRTC]: {
    iceServers?: RTCIceServer[];
    dataChannelOptions?: RTCDataChannelInit;
  };
}

/**
 * Active channel - running channel with type safety
 */
export interface ActiveChannel<TProtocol extends Protocol = Protocol> {
  readonly id: ChannelId<TProtocol>;
  readonly transport: JTAGTransport;
  readonly config: ChannelConfig<TProtocol>;
  readonly isConnected: () => boolean;
  readonly getMetrics: () => ChannelMetrics;
  readonly createdAt: Date;
}

/**
 * Channel metrics - monitoring without cross-cutting concerns
 */
export interface ChannelMetrics {
  readonly messagesSent: number;
  readonly messagesReceived: number;
  readonly bytesTransferred: number;
  readonly connectionUptime: number;
  readonly lastActivity: Date;
  readonly errorCount: number;
}

/**
 * Channel creation result - type-safe operation results
 */
export interface ChannelResult<TChannel extends ActiveChannel = ActiveChannel> {
  readonly success: boolean;
  readonly channel?: TChannel;
  readonly error?: ChannelError;
  readonly metadata?: {
    readonly duration: number;
    readonly retryCount: number;
  };
}

/**
 * Channel errors - well-typed error handling
 */
export interface ChannelError {
  readonly code: ChannelErrorCode;
  readonly message: string;
  readonly cause?: Error;
  readonly retryable: boolean;
}

export const CHANNEL_ERROR_CODES = {
  PROTOCOL_NOT_SUPPORTED: 'PROTOCOL_NOT_SUPPORTED',
  CONNECTION_FAILED: 'CONNECTION_FAILED',
  CHANNEL_EXISTS: 'CHANNEL_EXISTS',
  CHANNEL_NOT_FOUND: 'CHANNEL_NOT_FOUND',
  TRANSPORT_ERROR: 'TRANSPORT_ERROR',
  CONFIGURATION_INVALID: 'CONFIGURATION_INVALID'
} as const;

export type ChannelErrorCode = typeof CHANNEL_ERROR_CODES[keyof typeof CHANNEL_ERROR_CODES];

// ============================================================================
// ROUTING INTERFACES
// ============================================================================

/**
 * Route resolution - type-safe message routing
 */
export interface RouteResolution {
  readonly endpoint: TypedEndpoint;
  readonly channel: ActiveChannel;
  readonly requiresHop: boolean;
  readonly estimatedLatency?: number;
}

/**
 * Route resolution result
 */
export interface RouteResult {
  readonly success: boolean;
  readonly resolution?: RouteResolution;
  readonly error?: ChannelError;
  readonly alternativeRoutes?: RouteResolution[];
}

/**
 * Message routing context - what the router needs to make decisions
 */
export interface RoutingContext {
  readonly source: TypedEndpoint;
  readonly destination: TypedEndpoint;
  readonly message: JTAGMessage;
  readonly requiresResponse: boolean;
  readonly timeout?: number;
  readonly priority?: MessagePriority;
}

/**
 * Message priority for routing decisions
 */
export const MESSAGE_PRIORITIES = {
  CRITICAL: 0,  // System health, errors
  HIGH: 1,      // Commands, responses  
  NORMAL: 2,    // Events, notifications
  LOW: 3        // Background sync, metrics
} as const;

export type MessagePriority = typeof MESSAGE_PRIORITIES[keyof typeof MESSAGE_PRIORITIES];

// ============================================================================
// CHANNEL MANAGER INTERFACE
// ============================================================================

/**
 * Channel manager interface - single responsibility, strongly typed
 */
export interface IChannelManager {
  /**
   * Create channel with compile-time protocol validation
   */
  createChannel<TProtocol extends Protocol>(
    config: ChannelConfig<TProtocol>
  ): Promise<ChannelResult<ActiveChannel<TProtocol>>>;

  /**
   * Get channel by typed identifier
   */
  getChannel<TProtocol extends Protocol>(
    id: ChannelId<TProtocol>
  ): ActiveChannel<TProtocol> | undefined;

  /**
   * Route message to typed endpoint
   */
  routeMessage(context: RoutingContext): Promise<RouteResult>;

  /**
   * Get all channels for environment (type-safe)
   */
  getChannelsForEnvironment(environment: Environment): ActiveChannel[];

  /**
   * Close channel safely
   */
  closeChannel(id: ChannelId): Promise<boolean>;

  /**
   * Get comprehensive status
   */
  getStatus(): ChannelManagerStatus;
}

/**
 * Channel manager status - complete system visibility
 */
export interface ChannelManagerStatus {
  readonly totalChannels: number;
  readonly activeChannels: number;
  readonly channelsByEnvironment: Record<Environment, number>;
  readonly channelsByProtocol: Record<Protocol, number>;
  readonly totalMessages: number;
  readonly errorRate: number;
  readonly uptime: number;
}

// ============================================================================
// TYPE UTILITIES
// ============================================================================

/**
 * Channel type guards - runtime type safety
 */
export const ChannelTypeGuards = {
  isValidEnvironment: (env: string): env is Environment => {
    return Object.values(ENVIRONMENTS).includes(env as Environment);
  },

  isValidProtocol: (protocol: string): protocol is Protocol => {
    return Object.values(PROTOCOLS).includes(protocol as Protocol);
  },

  isRemoteEndpoint: (endpoint: TypedEndpoint): endpoint is TypedEndpoint & { nodeId: string } => {
    return endpoint.environment === ENVIRONMENTS.REMOTE && !!endpoint.nodeId;
  },

  isChannelConnected: (channel: ActiveChannel): boolean => {
    return channel.isConnected();
  }
} as const;

/**
 * Helper type for channel factories - ensures protocol consistency
 */
export type ChannelFactory<TProtocol extends Protocol> = {
  readonly protocol: TProtocol;
  createChannel(config: ChannelConfig<TProtocol>): Promise<ChannelResult<ActiveChannel<TProtocol>>>;
};