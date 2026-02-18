/**
 * UDP Multicast Transport Types
 * 
 * Shared types and interfaces for UDP multicast P2P networking.
 * Enables automatic node discovery and mesh communication.
 */

import type { JTAGMessage, JTAGPayload } from '../../../core/types/JTAGTypes';
import type { UUID } from '../../../core/types/CrossPlatformUUID';

/**
 * Node types in P2P mesh network
 */
export enum NodeType {
  SERVER = 'server',
  BROWSER = 'browser', 
  MOBILE = 'mobile',
  AI_AGENT = 'ai-agent'
}

/**
 * P2P node capabilities - what services this node provides
 */
export enum NodeCapability {
  SCREENSHOT = 'screenshot',
  FILE_OPERATIONS = 'file-operations',
  COMPILATION = 'compilation',
  BROWSER_AUTOMATION = 'browser-automation',
  AI_PROCESSING = 'ai-processing',
  DATA_STORAGE = 'data-storage'
}

/**
 * Discovery message types for node announcement and lookup
 */
export enum DiscoveryMessageType {
  NODE_ANNOUNCE = 'node-announce',
  NODE_QUERY = 'node-query',
  NODE_RESPONSE = 'node-response',
  NODE_HEARTBEAT = 'node-heartbeat',
  NODE_GOODBYE = 'node-goodbye'
}

/**
 * P2P Node information
 */
export interface P2PNodeInfo {
  readonly nodeId: string;
  readonly nodeType: NodeType;
  readonly capabilities: readonly NodeCapability[];
  readonly endpoints: {
    readonly unicastPort: number;
    readonly multicastAddress: string;
    readonly multicastPort: number;
  };
  readonly metadata: {
    readonly version: string;
    readonly hostname: string;
    readonly platform: string;
    readonly lastSeen: string;
  };
}

/**
 * Discovery protocol messages
 */
export interface DiscoveryMessage {
  readonly type: DiscoveryMessageType;
  readonly nodeInfo: P2PNodeInfo;
  readonly timestamp: string;
  readonly messageId: string;
  readonly payload?: Record<string, unknown>;
}

/**
 * UDP Multicast configuration
 */
export interface UDPMulticastConfig {
  readonly nodeId: string;
  readonly nodeType: NodeType;
  readonly capabilities: readonly NodeCapability[];
  readonly multicastAddress: string;
  readonly multicastPort: number;
  readonly unicastPort: number;
  readonly ttl: number;
  readonly discoveryInterval: number;
  readonly heartbeatInterval: number;
  readonly nodeTimeout: number;
  readonly encryptionKey?: string;
}

/**
 * P2P message with routing information
 * Uses intersection with JTAGMessage to maintain strong typing
 */
export type P2PMessage<T extends JTAGPayload = JTAGPayload> = JTAGMessage<T> & {
  p2p: {
    sourceNodeId: string;
    targetNodeId?: string; // undefined = broadcast
    routingPath: string[];
    hops: number;
    maxHops: number;
  };
}

/**
 * Network topology information
 */
export interface NetworkTopology {
  readonly nodes: Record<string, P2PNodeInfo>;
  readonly routes: Record<string, readonly string[]>;
  readonly lastUpdated: string;
}

/**
 * UDP Transport statistics
 */
export interface UDPTransportStats {
  messagesRx: number;
  messagesTx: number;
  bytesRx: number;
  bytesTx: number;
  nodesDiscovered: number;
  activeNodes: number;
  lastActivity: string;
}

/**
 * Default configuration values
 */
export const UDP_MULTICAST_DEFAULTS = {
  MULTICAST_ADDRESS: '239.192.74.71', // JTAG reserved multicast group
  MULTICAST_PORT: 37471,
  TTL: 32,
  DISCOVERY_INTERVAL: 30000, // 30 seconds
  HEARTBEAT_INTERVAL: 15000,  // 15 seconds  
  NODE_TIMEOUT: 60000,        // 1 minute
  MAX_HOPS: 8,
  MAX_MESSAGE_SIZE: 65536     // 64KB
} as const;

/**
 * Protocol magic numbers for message identification
 */
export const PROTOCOL_MAGIC = {
  DISCOVERY: 'JTAG_DISC',
  P2P_MESSAGE: 'JTAG_P2P'
} as const;