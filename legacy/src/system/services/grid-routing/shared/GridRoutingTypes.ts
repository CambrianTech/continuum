/**
 * Grid Routing Types
 * 
 * Core types for The Grid P2P routing and discovery system.
 * This is the backbone transport layer that enables location-transparent
 * communication across The Grid mesh network.
 * 
 * Built step-by-step, modularly, with proper validation and testing.
 * No shortcuts - this is the foundation of Continuum's nervous system.
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';

/**
 * Node identity and basic information
 */
export interface GridNodeIdentity {
  readonly nodeId: UUID;
  readonly nodeType: 'server' | 'browser' | 'mobile';
  readonly hostname: string;
  readonly version: string;
  readonly capabilities: readonly string[];
}

/**
 * Node network endpoints and connection info
 */
export interface GridNodeEndpoints {
  readonly multicastAddress: string;
  readonly multicastPort: number;
  readonly unicastPort: number;
  readonly httpPort?: number;
  readonly webSocketPort?: number;
}

/**
 * Complete node information for routing table
 */
export interface GridNode {
  readonly identity: GridNodeIdentity;
  readonly endpoints: GridNodeEndpoints;
  readonly metadata: GridNodeMetadata;
  readonly status: GridNodeStatus;
}

/**
 * Node metadata for routing decisions
 */
export interface GridNodeMetadata {
  readonly platform: string;
  readonly region: string;
  readonly loadLevel: number;        // 0.0 to 1.0
  readonly reliability: number;      // 0.0 to 1.0 based on history
  readonly latency: number;          // Average latency in ms
  readonly lastSeen: string;         // ISO timestamp
}

/**
 * Node operational status
 */
export interface GridNodeStatus {
  readonly isOnline: boolean;
  readonly isReachable: boolean;
  readonly connectionCount: number;
  readonly uptimeSeconds: number;
  readonly memoryUsage: number;      // MB
}

/**
 * Routing table entry
 */
export interface RoutingEntry {
  readonly targetNodeId: UUID;
  readonly nextHopNodeId: UUID;
  readonly distance: number;         // Number of hops
  readonly cost: number;             // Routing cost metric
  readonly path: readonly UUID[];    // Full path to target
  readonly updatedAt: string;        // ISO timestamp
  readonly reliability: number;      // Path reliability score
}

/**
 * Complete routing table for a node
 */
export interface RoutingTable {
  readonly localNodeId: UUID;
  readonly entries: ReadonlyMap<UUID, RoutingEntry>;
  readonly lastUpdated: string;
  readonly version: number;
}

/**
 * Grid transport message types - DUMB PIPE ONLY
 * Transport layer has no knowledge of message payload content
 */
export enum GridMessageType {
  // Node lifecycle
  NODE_ANNOUNCE = 'node-announce',
  NODE_HEARTBEAT = 'node-heartbeat',
  NODE_GOODBYE = 'node-goodbye',
  
  // Routing protocol
  ROUTING_UPDATE = 'routing-update',
  ROUTING_REQUEST = 'routing-request',
  ROUTING_RESPONSE = 'routing-response',
  
  // Generic message forwarding (payload-agnostic)
  FORWARD_MESSAGE = 'forward-message',
  
  // Network maintenance
  PING_REQUEST = 'ping-request',
  PING_RESPONSE = 'ping-response',
  HEALTH_CHECK = 'health-check'
}

/**
 * Base Grid message structure
 */
export interface BaseGridMessage {
  readonly messageId: UUID;
  readonly type: GridMessageType;
  readonly sourceNodeId: UUID;
  readonly targetNodeId?: UUID;      // undefined = broadcast
  readonly timestamp: string;        // ISO timestamp
  readonly ttl: number;              // Time to live (hops)
  readonly priority: number;         // 0-10, higher = more priority
}

/**
 * Node announcement message (for discovery)
 */
export interface NodeAnnounceMessage extends BaseGridMessage {
  readonly type: GridMessageType.NODE_ANNOUNCE;
  readonly payload: {
    readonly node: GridNode;
    readonly routingTable: RoutingEntry[];  // Sharing known routes
  };
}

/**
 * Heartbeat message (keep-alive)
 */
export interface NodeHeartbeatMessage extends BaseGridMessage {
  readonly type: GridMessageType.NODE_HEARTBEAT;
  readonly payload: {
    readonly status: GridNodeStatus;
    readonly routingUpdates: RoutingEntry[];  // Recent routing changes
  };
}

/**
 * Routing update message (share routing info)
 */
export interface RoutingUpdateMessage extends BaseGridMessage {
  readonly type: GridMessageType.ROUTING_UPDATE;
  readonly payload: {
    readonly updates: RoutingEntry[];
    readonly removals: UUID[];              // Nodes to remove from routing
    readonly fullTable: boolean;            // Is this a complete table dump?
  };
}

/**
 * Generic message forwarding - PAYLOAD AGNOSTIC
 * Transport has no knowledge of what the payload contains
 */
export interface ForwardMessage extends BaseGridMessage {
  readonly type: GridMessageType.FORWARD_MESSAGE;
  readonly payload: unknown;              // Completely opaque to transport
}

/**
 * Union type for all Grid messages - TRANSPORT LAYER ONLY
 * No command-specific knowledge
 */
export type GridMessage = 
  | NodeAnnounceMessage
  | NodeHeartbeatMessage
  | RoutingUpdateMessage
  | ForwardMessage;

/**
 * Message routing information
 */
export interface MessageRoute {
  readonly messageId: UUID;
  readonly originalSource: UUID;
  readonly finalTarget: UUID;
  readonly currentHop: UUID;
  readonly nextHop: UUID;
  readonly hopCount: number;
  readonly maxHops: number;
  readonly routingPath: readonly UUID[];
}

/**
 * Grid network topology snapshot
 */
export interface GridTopology {
  readonly nodes: ReadonlyMap<UUID, GridNode>;
  readonly routingTables: ReadonlyMap<UUID, RoutingTable>;
  readonly connections: ReadonlyMap<UUID, readonly UUID[]>;  // node -> connected nodes
  readonly lastUpdated: string;
  readonly version: number;
}

/**
 * Discovery query for finding nodes
 */
export interface NodeDiscoveryQuery {
  readonly capabilities?: readonly string[];
  readonly nodeType?: 'server' | 'browser' | 'mobile';
  readonly maxLatency?: number;        // ms
  readonly minReliability?: number;    // 0.0 to 1.0
  readonly region?: string;
  readonly maxHops?: number;
}

/**
 * Discovery result
 */
export interface NodeDiscoveryResult {
  readonly node: GridNode;
  readonly route: RoutingEntry;
  readonly matchScore: number;         // How well this matches the query
}

/**
 * Grid statistics and metrics
 */
export interface GridStatistics {
  readonly nodeCount: number;
  readonly activeConnections: number;
  readonly avgLatency: number;
  readonly messagesSent: number;
  readonly messagesReceived: number;
  readonly routingTableSize: number;
  readonly networkDiameter: number;    // Max hops between any two nodes
  readonly uptimePercentage: number;
}

/**
 * Configuration for Grid routing
 */
export interface GridRoutingConfig {
  readonly nodeId: UUID;
  readonly nodeType: 'server' | 'browser' | 'mobile';
  readonly announceInterval: number;    // ms between announcements
  readonly heartbeatInterval: number;   // ms between heartbeats
  readonly routingUpdateInterval: number; // ms between routing updates
  readonly maxHops: number;
  readonly nodeTimeout: number;         // ms before considering node dead
  readonly maxRoutingTableSize: number;
  readonly enableLogging: boolean;
}

/**
 * Default Grid configuration
 */
export const GRID_ROUTING_DEFAULTS = {
  ANNOUNCE_INTERVAL: 30000,      // 30 seconds
  HEARTBEAT_INTERVAL: 15000,     // 15 seconds  
  ROUTING_UPDATE_INTERVAL: 60000, // 1 minute
  MAX_HOPS: 8,
  NODE_TIMEOUT: 90000,           // 90 seconds
  MAX_ROUTING_TABLE_SIZE: 1000,
  DEFAULT_PRIORITY: 5,
  MAX_TTL: 16,
  MESSAGE_TIMEOUT: 30000         // 30 seconds
} as const;

/**
 * Error types for Grid operations
 */
export enum GridErrorType {
  NODE_NOT_FOUND = 'node-not-found',
  ROUTE_NOT_FOUND = 'route-not-found',
  MESSAGE_TIMEOUT = 'message-timeout',
  INVALID_MESSAGE = 'invalid-message',
  PERMISSION_DENIED = 'permission-denied',
  NETWORK_ERROR = 'network-error',
  COMMAND_FAILED = 'command-failed'
}

/**
 * Grid operation error
 */
export interface GridError {
  readonly type: GridErrorType;
  readonly message: string;
  readonly nodeId?: UUID;
  readonly messageId?: UUID;
  readonly details?: Record<string, unknown>;
}

/**
 * Result wrapper for Grid operations
 */
export type GridResult<T> = {
  readonly success: true;
  readonly data: T;
} | {
  readonly success: false;
  readonly error: GridError;
};

/**
 * Async iterator for streaming responses
 */
export interface GridMessageStream {
  readonly messageId: UUID;
  readonly sourceNode: UUID;
  [Symbol.asyncIterator](): AsyncIterator<GridMessage>;
  close(): Promise<void>;
}

/**
 * Grid event types for event system
 */
export enum GridEventType {
  NODE_JOINED = 'node-joined',
  NODE_LEFT = 'node-left',
  ROUTE_DISCOVERED = 'route-discovered',
  ROUTE_LOST = 'route-lost',
  MESSAGE_RECEIVED = 'message-received',
  COMMAND_EXECUTED = 'command-executed',
  TOPOLOGY_CHANGED = 'topology-changed'
}

/**
 * Grid event payload
 */
export interface GridEvent {
  readonly type: GridEventType;
  readonly timestamp: string;
  readonly nodeId?: UUID;
  readonly data: unknown;
}