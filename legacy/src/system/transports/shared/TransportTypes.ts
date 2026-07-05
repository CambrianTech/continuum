/**
 * Transport Types - Shared interfaces and types for transport system
 * 
 * Extracted from TransportFactory megafile into proper modular structure.
 * Provides core transport interfaces that all transport implementations must follow.
 */

import type { JTAGMessage } from '../../core/types/JTAGTypes';
import type { EventsInterface } from '../../events';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { ITransportHandler } from './ITransportHandler';

/**
 * Transport roles - defines the connection behavior and capabilities
 * 
 * ROLE BEHAVIORS:
 * 
 * CLIENT: 
 * - Initiates outbound connections to servers
 * - Used by: CLI, browser JTAGRouter connecting to server
 * - Connection: One-to-one (client → server)
 * 
 * SERVER:
 * - Listens for inbound connections from clients  
 * - Used by: Server JTAGRouter, main system listener
 * - Connection: One-to-many (server ← multiple clients)
 * 
 * PEER:
 * - Bidirectional communication in mesh network
 * - Can both connect to and accept connections from other peers
 * - Used by: P2P mesh nodes, distributed systems
 * - Connection: Many-to-many (peer ↔ peer ↔ peer)
 * 
 * RELAY:
 * - Routes/forwards messages between other transports
 * - Acts as intermediary for network bridging
 * - Used by: Gateway nodes, network bridges
 * - Connection: Hub pattern (multiple ← relay → multiple)
 * 
 * HYBRID:
 * - Combines client + server capabilities
 * - Can simultaneously connect outbound and accept inbound
 * - Used by: Advanced distributed architectures
 * - Connection: Full mesh capabilities
 */
export const TRANSPORT_ROLES = {
  CLIENT: 'client',
  SERVER: 'server', 
  PEER: 'peer'
} as const;

export type TransportRole = typeof TRANSPORT_ROLES[keyof typeof TRANSPORT_ROLES];

export type TransportProtocol = 'websocket' | 'http' | 'udp-multicast';


/**
 * Transport configuration - simplified payload-based architecture
 * Follows same pattern as other JTAG config interfaces
 */
export interface TransportConfig {
  protocol: TransportProtocol;                         // REQUIRED - which transport to use
  role: TransportRole;                                 // REQUIRED - client or server behavior
  eventSystem: EventsInterface;                        // REQUIRED - event handling
  sessionId: UUID;                                     // REQUIRED - session identification
  serverPort?: number;                                 // OPTIONAL - required for server role
  serverUrl?: string;                                  // OPTIONAL - required for client role  
  fallback?: boolean;                                  // OPTIONAL - enable fallback transport
  handler: ITransportHandler;                          // REQUIRED - transport protocol handler
  // UDP multicast specific options
  p2p?: {
    nodeId?: string;
    nodeType?: 'server' | 'browser' | 'mobile' | 'ai-agent';
    capabilities?: string[];
    multicastAddress?: string;
    multicastPort?: number;
    unicastPort?: number;
    encryptionKey?: string;
  };
}

/**
 * Transport send result interface
 */
export interface TransportSendResult {
  success: boolean;
  timestamp: string;
  sentCount?: number;
}

/**
 * JTAG Transport Interface
 * 
 * Abstraction for cross-context message delivery mechanisms.
 * Implementations include WebSocket, HTTP, and UDP multicast transports.
 * 
 * PAYLOAD-BASED: Same pattern as other JTAG interfaces
 */
export interface JTAGTransport {
  name: string;
  send(message: JTAGMessage): Promise<TransportSendResult>;
  isConnected(): boolean;
  disconnect(): Promise<void>;
  reconnect?(): Promise<void>;
  setMessageHandler?(handler: (message: JTAGMessage) => void): void;
}

/**
 * Standard transport type categories for router management
 */
export enum TRANSPORT_TYPES {
  CROSS_CONTEXT = 'cross-context',
  P2P = 'p2p',  
}