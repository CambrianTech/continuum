/**
 * Pure Transport Types - Clean interfaces for dumb transport pipes
 * 
 * These interfaces define truly pure transport functionality - only data movement.
 * No JTAG business logic, no session handling, no message routing - just pipes.
 * 
 * ARCHITECTURE PRINCIPLE: "Transports are dumb pipes, configuration is the blueprint"
 */

/**
 * Pure transport protocols - TypeScript enforced
 */
export const PURE_TRANSPORT_PROTOCOLS = {
  WEBSOCKET: 'websocket',
  HTTP: 'http', 
  UDP_MULTICAST: 'udp-multicast'
} as const;

export type PureTransportProtocol = typeof PURE_TRANSPORT_PROTOCOLS[keyof typeof PURE_TRANSPORT_PROTOCOLS];

/**
 * Pure transport connection configuration
 * Only contains networking parameters - no business logic
 * TypeScript enforces protocol safety
 */
export interface PureTransportConfig {
  // Protocol-specific parameters - TypeScript enforced
  protocol: PureTransportProtocol;
  
  // Connection parameters only
  host?: string;
  port?: number;
  url?: string;
  
  // Network behavior
  timeout?: number;
  retryAttempts?: number;
  retryDelay?: number;
  
  // UDP-specific
  multicastAddress?: string;
  multicastPort?: number;
  unicastPort?: number;
  
  // No JTAG concepts - no eventSystem, no handler, no sessionId
}

/**
 * Pure transport interface - truly dumb pipes
 * Only send/receive raw data - no message interpretation
 * TypeScript enforces protocol contracts
 */
export interface PureTransport {
  readonly name: string;
  readonly protocol: PureTransportProtocol;
  
  // Core pipe functionality
  send(data: string | Uint8Array): Promise<PureSendResult>;
  isConnected(): boolean;
  connect(config?: PureTransportConfig): Promise<void>;
  disconnect(): Promise<void>;
  
  // Pure data callbacks - no message interpretation
  onData?(callback: (data: string | Uint8Array) => void): void;
  onConnect?(callback: () => void): void;
  onDisconnect?(callback: (reason?: string) => void): void;
  onError?(callback: (error: Error) => void): void;
  
  // No JTAG concepts - no setMessageHandler, no eventSystem integration
}

/**
 * Pure send result - only transport-level information
 */
export interface PureSendResult {
  success: boolean;
  timestamp: string;
  bytesTransmitted?: number;
  // No business logic results
}

/**
 * Pure transport factory interface
 * Creates transports based on pure configuration
 */
export interface PureTransportFactory {
  create(config: PureTransportConfig): Promise<PureTransport>;
  supports(protocol: string): boolean;
}