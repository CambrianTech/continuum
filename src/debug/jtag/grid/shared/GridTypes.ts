/**
 * The Grid - Mesh P2P System Types
 * 
 * "I fight for the Users!" - Flynn
 * 
 * The Grid is a distributed mesh network system where JTAG nodes automatically
 * discover each other and form a resilient P2P network for distributed command
 * execution. Inspired by Flynn's vision of a liberated digital frontier.
 */

import type { NodeType, NodeCapability, P2PNodeInfo } from '../../system/transports/udp-multicast-transport/shared/UDPMulticastTypes';
import type { JTAGMessage, JTAGPayload } from '../../system/core/types/JTAGTypes';

/**
 * Grid Node Status - State of a node in The Grid
 */
export enum GridNodeStatus {
  INITIALIZING = 'initializing',
  DISCOVERING = 'discovering', 
  CONNECTED = 'connected',
  ACTIVE = 'active',
  DEGRADED = 'degraded',
  OFFLINE = 'offline'
}

/**
 * Grid Network Roles
 */
export enum GridRole {
  USER_NODE = 'user-node',          // Developer workstation
  BUILD_NODE = 'build-node',        // CI/CD and compilation
  AI_NODE = 'ai-node',             // AI processing node
  STORAGE_NODE = 'storage-node',   // Data storage and persistence
  GATEWAY_NODE = 'gateway-node',   // Internet/external gateway
  RELAY_NODE = 'relay-node'        // Network relay and routing
}

/**
 * Extended Grid Node Information
 */
export interface GridNode extends P2PNodeInfo {
  readonly gridRole: GridRole;
  readonly status: GridNodeStatus;
  readonly load: {
    readonly cpu: number;        // 0-100%
    readonly memory: number;     // 0-100%
    readonly network: number;    // 0-100%
    readonly connections: number;
  };
  readonly reputation: number;   // 0-100 trust score
  readonly uptime: number;       // milliseconds
}

/**
 * Grid Command with automatic node selection
 */
export interface GridCommand<T extends JTAGPayload = JTAGPayload> {
  readonly command: string;
  readonly payload: T;
  readonly requirements: {
    readonly capabilities: readonly NodeCapability[];
    readonly minReputation?: number;
    readonly maxLoad?: number;
    readonly excludeNodes?: readonly string[];
    readonly preferNodes?: readonly string[];
  };
  readonly routing: {
    readonly strategy: GridRoutingStrategy;
    readonly maxHops?: number;
    readonly timeout?: number;
    readonly retries?: number;
  };
}

/**
 * Grid routing strategies
 */
export enum GridRoutingStrategy {
  OPTIMAL = 'optimal',           // Choose best node based on load/reputation
  BROADCAST = 'broadcast',       // Send to all capable nodes
  FAILOVER = 'failover',        // Try nodes in order until success
  ROUND_ROBIN = 'round-robin',   // Distribute load evenly
  LOCAL_FIRST = 'local-first',   // Prefer local subnet nodes
  FASTEST = 'fastest'            // Choose lowest latency node
}

/**
 * Grid Network Topology
 */
export interface GridTopology {
  readonly nodes: Record<string, GridNode>;
  readonly connections: Record<string, readonly string[]>;
  readonly clusters: {
    readonly [subnetId: string]: {
      readonly nodes: readonly string[];
      readonly gateway?: string;
    };
  };
  readonly routes: Record<string, readonly string[]>;
  readonly lastUpdated: string;
  readonly networkId: string;
}

/**
 * Grid Health Metrics
 */
export interface GridHealth {
  readonly nodeCount: number;
  readonly activeNodes: number;
  readonly totalConnections: number;
  readonly averageLatency: number;
  readonly networkLoad: number;
  readonly faultTolerance: number;  // 0-100%
  readonly lastHealthCheck: string;
}

/**
 * Grid Events
 */
export enum GridEventType {
  NODE_JOINED = 'node-joined',
  NODE_LEFT = 'node-left',
  NODE_STATUS_CHANGED = 'node-status-changed',
  TOPOLOGY_CHANGED = 'topology-changed',
  COMMAND_ROUTED = 'command-routed',
  NETWORK_PARTITION = 'network-partition',
  NETWORK_HEALED = 'network-healed'
}

/**
 * Grid Event
 */
export interface GridEvent<T = unknown> {
  readonly type: GridEventType;
  readonly nodeId: string;
  readonly timestamp: string;
  readonly data: T;
}

/**
 * Grid Configuration
 */
export interface GridConfig {
  readonly nodeId: string;
  readonly gridRole: GridRole;
  readonly capabilities: readonly NodeCapability[];
  readonly networking: {
    readonly multicastAddress: string;
    readonly multicastPort: number;
    readonly unicastPort: number;
    readonly maxConnections: number;
  };
  readonly discovery: {
    readonly announceInterval: number;
    readonly healthCheckInterval: number;
    readonly nodeTimeout: number;
  };
  readonly routing: {
    readonly defaultStrategy: GridRoutingStrategy;
    readonly maxHops: number;
    readonly timeout: number;
    readonly retries: number;
  };
  readonly security: {
    readonly encryptionKey?: string;
    readonly trustedNodes?: readonly string[];
    readonly allowedSubnets?: readonly string[];
  };
}

/**
 * Grid Command Result
 */
export interface GridCommandResult<T = unknown> {
  readonly success: boolean;
  readonly result?: T;
  readonly error?: string;
  readonly executedOn: string;
  readonly route: readonly string[];
  readonly latency: number;
  readonly timestamp: string;
}

/**
 * Default Grid Configuration Values
 */
export const GRID_DEFAULTS = {
  ANNOUNCE_INTERVAL: 30000,        // 30 seconds
  HEALTH_CHECK_INTERVAL: 15000,    // 15 seconds  
  NODE_TIMEOUT: 60000,             // 1 minute
  MAX_CONNECTIONS: 50,
  MAX_HOPS: 8,
  DEFAULT_TIMEOUT: 30000,          // 30 seconds
  DEFAULT_RETRIES: 3,
  MIN_REPUTATION: 50
} as const;