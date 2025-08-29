/**
 * Connection Broker Types - Core interfaces for intelligent connection management
 * 
 * The Connection Broker provides centralized, globally-aware transport orchestration
 * that eliminates port conflicts and enables intelligent server reuse across multiple
 * client connections.
 * 
 * Key Principles:
 * - Location Transparency: Same API works for local, remote, and P2P connections
 * - Connection Reuse: Multiple clients can share the same server when appropriate
 * - Port Intelligence: Dynamic port allocation with conflict detection
 * - Strong Typing: Full TypeScript type safety with no 'any' types
 * - Dual Addressing: Support both GUID and human-readable server names
 */

import type { UUID } from '../../types/CrossPlatformUUID';
import type { JTAGContext, JTAGEnvironment } from '../../types/JTAGTypes';
import type { TransportProtocol, JTAGTransport } from '../../../transports/shared/TransportTypes';
import type { NodeCapability } from '../../../transports/udp-multicast-transport/shared/UDPMulticastTypes';
import type { EventsInterface } from '../../../events/shared/JTAGEventSystem';
import type { ITransportHandler } from '../../../transports/shared/ITransportHandler';

/**
 * Connection requirement specification - what the client needs
 * This normalizes all the different connection scenarios into a single interface
 */
export interface ConnectionParams {
  /** Connection protocol preference order (tries first available) */
  readonly protocols: readonly TransportProtocol[];
  
  /** Connection sharing mode */
  readonly mode: ConnectionMode;
  
  /** Target execution environment */
  readonly targetEnvironment: JTAGEnvironment;
  
  /** Session identifier for this client */
  readonly sessionId: UUID;
  
  /** Client context information */
  readonly context: JTAGContext;
  
  /** Event system for transport communication (strongly typed) */
  readonly eventSystem: EventsInterface;
  
  /** Transport message handler (strongly typed) */
  readonly handler: ITransportHandler;
  
  /** Server selection criteria - flexible addressing */
  readonly server?: ServerSelector;
  
  /** Connection timeout in milliseconds */
  readonly timeoutMs?: number;
  
  /** Enable automatic fallback to alternative protocols */
  readonly enableFallback?: boolean;
  
  /** Maximum connection retry attempts */
  readonly maxRetries?: number;
  
  /** P2P-specific capabilities (for mesh networking) */
  readonly capabilities?: readonly NodeCapability[];
  
  /** Custom connection metadata */
  readonly metadata?: Record<string, unknown>;
}

/**
 * Connection sharing strategies
 */
export type ConnectionMode = 
  | 'shared'    // Reuse existing compatible server if available
  | 'isolated'  // Always create new server instance
  | 'preferred' // Prefer shared, fall back to isolated
  | 'required'; // Must reuse existing server (fail if none available)

/**
 * Server selection criteria - supports both GUID and name-based addressing
 * Similar to HTML element id/name pattern for flexible addressing
 */
export interface ServerSelector {
  /** Server GUID (unique, immutable identifier) */
  readonly guid?: UUID;
  
  /** Human-readable server name (mutable, user-friendly) */
  readonly name?: string;
  
  /** Preferred port (hint for server selection) */
  readonly port?: number;
  
  /** Additional server tags for filtering */
  readonly tags?: readonly string[];
}

/**
 * Server registry entry - complete server information
 */
export interface ServerRegistryEntry {
  /** Unique server identifier */
  readonly guid: UUID;
  
  /** Human-readable name (can be updated) */
  name: string;
  
  /** Server listening port */
  readonly port: number;
  
  /** Transport protocol */
  readonly protocol: TransportProtocol;
  
  /** Server execution environment */
  readonly environment: JTAGEnvironment;
  
  /** Creation timestamp */
  readonly createdAt: Date;
  
  /** Last activity timestamp */
  lastActivity: Date;
  
  /** Current connection count */
  connectionCount: number;
  
  /** Server capabilities (for P2P mesh) */
  readonly capabilities?: readonly NodeCapability[];
  
  /** Server status */
  status: ServerStatus;
  
  /** Associated process ID (for cleanup) */
  readonly processId?: number;
  
  /** Custom server metadata */
  readonly metadata?: Record<string, unknown>;
  
  /** Server tags for categorization */
  readonly tags?: readonly string[];
}

/**
 * Server lifecycle status
 */
export type ServerStatus = 
  | 'starting'    // Server is initializing
  | 'ready'       // Server is accepting connections
  | 'busy'        // Server at capacity (for isolated servers)
  | 'draining'    // Server shutting down gracefully
  | 'stopped'     // Server no longer available
  | 'error';      // Server in error state

/**
 * Connection result - describes successful connection establishment
 */
export interface ConnectionResult {
  /** Established transport connection */
  readonly transport: JTAGTransport;
  
  /** Server that handled the connection */
  readonly server: ServerRegistryEntry;
  
  /** Connection establishment strategy used */
  readonly strategy: ConnectionStrategy;
  
  /** Connection metadata */
  readonly metadata: ConnectionMetadata;
}

/**
 * Connection establishment strategies
 */
export type ConnectionStrategy = 
  | 'reused_existing'     // Connected to existing compatible server
  | 'created_new'         // Created new server instance
  | 'discovered_p2p'      // Found server via P2P discovery
  | 'fallback_protocol';  // Used fallback protocol

/**
 * Connection establishment metadata
 */
export interface ConnectionMetadata {
  /** Time taken to establish connection (ms) */
  readonly establishmentTimeMs: number;
  
  /** Number of retry attempts needed */
  readonly retryAttempts: number;
  
  /** Protocol used (may differ from requested) */
  readonly protocolUsed: TransportProtocol;
  
  /** Whether fallback was triggered */
  readonly usedFallback: boolean;
  
  /** Additional diagnostic information */
  readonly diagnostics?: Record<string, unknown>;
}

/**
 * Port pool configuration
 */
export interface PortPoolConfig {
  /** Starting port number for dynamic allocation */
  readonly startPort: number;
  
  /** Ending port number for dynamic allocation */
  readonly endPort: number;
  
  /** Reserved ports that should not be allocated */
  readonly reservedPorts: readonly number[];
  
  /** Port allocation strategy */
  readonly allocationStrategy: PortAllocationStrategy;
}

/**
 * Port allocation strategies
 */
export type PortAllocationStrategy = 
  | 'sequential'  // Allocate ports in order (9001, 9002, 9003...)
  | 'random'      // Random port selection within range
  | 'round_robin' // Distribute ports evenly across range
  | 'least_used'; // Prefer ports with fewer historical allocations

/**
 * Connection broker configuration
 */
export interface ConnectionBrokerConfig {
  /** Port pool configuration */
  readonly portPool: PortPoolConfig;
  
  /** Server registry persistence settings */
  readonly registry: RegistryConfig;
  
  /** Connection timeout defaults */
  readonly timeouts: TimeoutConfig;
  
  /** P2P discovery settings */
  readonly p2pDiscovery?: P2PDiscoveryConfig;
}

/**
 * Registry persistence configuration
 */
export interface RegistryConfig {
  /** Registry file path */
  readonly filePath?: string;
  
  /** Registry persistence interval (ms) */
  readonly persistInterval: number;
  
  /** Registry cleanup interval (ms) */
  readonly cleanupInterval: number;
  
  /** Maximum server entry age before cleanup (ms) */
  readonly maxEntryAge: number;
}

/**
 * Timeout configuration
 */
export interface TimeoutConfig {
  /** Default connection timeout (ms) */
  readonly connection: number;
  
  /** Default retry delay (ms) */
  readonly retryDelay: number;
  
  /** Maximum total connection time (ms) */
  readonly maxTotal: number;
}

/**
 * P2P discovery configuration
 */
export interface P2PDiscoveryConfig {
  /** Multicast group address */
  readonly multicastAddress: string;
  
  /** Multicast port */
  readonly multicastPort: number;
  
  /** Discovery timeout (ms) */
  readonly discoveryTimeout: number;
  
  /** Maximum discovered nodes to track */
  readonly maxNodes: number;
}

/**
 * Connection broker error types
 */
export class ConnectionBrokerError extends Error {
  constructor(
    message: string,
    public readonly code: ConnectionErrorCode,
    public readonly context?: Record<string, unknown>
  ) {
    super(message);
    this.name = 'ConnectionBrokerError';
  }
}

/**
 * Connection error classifications
 */
export type ConnectionErrorCode = 
  | 'NO_AVAILABLE_PORTS'
  | 'SERVER_NOT_FOUND'
  | 'CONNECTION_TIMEOUT'
  | 'PROTOCOL_UNSUPPORTED'
  | 'REGISTRY_ERROR'
  | 'PORT_CONFLICT'
  | 'P2P_DISCOVERY_FAILED'
  | 'INVALID_CONFIGURATION';

/**
 * Connection broker interface - core functionality contract
 */
export interface IConnectionBroker {
  /**
   * Establish connection based on requirements
   * This is the main entry point for all client connections
   */
  connect(params: ConnectionParams): Promise<ConnectionResult>;
  
  /**
   * Register a new server in the broker
   * Called when new server instances are created
   */
  registerServer(server: Omit<ServerRegistryEntry, 'guid' | 'createdAt' | 'lastActivity' | 'connectionCount' | 'status'>): Promise<UUID>;
  
  /**
   * Unregister server from broker
   * Called when server instances shut down
   */
  unregisterServer(serverGuid: UUID): Promise<void>;
  
  /**
   * Find available servers matching criteria
   * Used for connection reuse and server discovery
   */
  findServers(selector?: ServerSelector, mode?: ConnectionMode): Promise<readonly ServerRegistryEntry[]>;
  
  /**
   * Get current server registry state
   * Useful for diagnostics and monitoring
   */
  getRegistryState(): Promise<{
    servers: readonly ServerRegistryEntry[];
    portAllocations: ReadonlyMap<number, UUID>;
    statistics: BrokerStatistics;
  }>;
  
  /**
   * Cleanup inactive servers and ports
   * Maintenance operation for long-running brokers
   */
  cleanup(): Promise<void>;
}

/**
 * Broker operational statistics
 */
export interface BrokerStatistics {
  /** Total connection attempts */
  totalConnections: number;
  
  /** Successful connections */
  successfulConnections: number;
  
  /** Connection reuse rate */
  reuseRate: number;
  
  /** Average connection establishment time (ms) */
  avgConnectionTime: number;
  
  /** Active servers by protocol */
  serversByProtocol: ReadonlyMap<TransportProtocol, number>;
  
  /** Port utilization statistics */
  portUtilization: {
    total: number;
    allocated: number;
    available: number;
  };
}

/**
 * Default broker configuration values
 */
export const DEFAULT_BROKER_CONFIG: ConnectionBrokerConfig = {
  portPool: {
    startPort: 9001,
    endPort: 9100,
    reservedPorts: [],
    allocationStrategy: 'sequential'
  },
  registry: {
    persistInterval: 5000,    // 5 seconds
    cleanupInterval: 30000,   // 30 seconds
    maxEntryAge: 300000       // 5 minutes
  },
  timeouts: {
    connection: 10000,        // 10 seconds
    retryDelay: 1000,         // 1 second
    maxTotal: 30000           // 30 seconds
  },
  p2pDiscovery: {
    multicastAddress: '239.255.255.250',
    multicastPort: 37471,
    discoveryTimeout: 5000,   // 5 seconds
    maxNodes: 50
  }
} as const;