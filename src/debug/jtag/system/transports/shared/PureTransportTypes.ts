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
 * Transport Request - What orchestrator asks for
 * 
 * Generic request format that works for any configuration source.
 * Factory translates this to specific destinations.
 */
export interface TransportRequest {
  protocol: string;                      // 'websocket', 'http', 'udp'
  role: 'client' | 'server';            // Connection behavior
  
  // Optional overrides - factory falls back to config
  host?: string;                         // Override default
  port?: number;                         // Override default
  path?: string;                         // For HTTP: /api/messages
  secure?: boolean;                      // Use TLS/SSL
  
  // Context for resolution
  environment: 'browser' | 'server';     // Factory uses appropriate config
  configSource: any;                     // Raw config (JTAGContext, etc.)
}

/**
 * Enhanced Transport Factory - Configuration to Destination Resolver
 * 
 * Responsible for:
 * - Reading configuration structures  
 * - Resolving destinations (URLs/addresses)
 * - Creating transports with resolved destinations
 * - Handling environment differences (browser vs server)
 */
export interface PureTransportFactory {
  // Legacy interface for backward compatibility
  create(config: PureTransportConfig): Promise<PureTransport>;
  supports(protocol: string): boolean;
  
  // New dumb pipe interface for radical reconfiguration
  createPureTransport(protocol: string, request: TransportRequest): Promise<PureTransport>;
  resolveDestination(protocol: string, request: TransportRequest): string;
  getSupportedProtocols(): string[];
}

/**
 * Transport Orchestrator - Business Logic Layer
 * 
 * Handles everything transports don't:
 * - Message serialization/deserialization
 * - Connection management and retry logic
 * - Health monitoring and failover
 * - Request/response correlation
 * - Event system integration
 */
export interface TransportOrchestrator {
  // High-level connection management  
  connect(request: TransportRequest): Promise<void>;
  disconnect(): Promise<void>;
  reconnect(): Promise<void>;
  
  // Message operations (handles serialization)
  sendMessage(message: any): Promise<void>;
  onMessage(handler: (message: any) => void): void;
  
  // Health and monitoring
  getHealth(): TransportHealth;
  onHealthChange(handler: (health: TransportHealth) => void): void;
}

/**
 * Transport Health - Monitoring data
 */
export interface TransportHealth {
  connected: boolean;
  lastActivity: Date;
  reconnectAttempts: number;
  bytesIn: number;
  bytesOut: number;
}

/**
 * Radical Reconfiguration Examples:
 * 
 * // Dynamic ports - no hardcoding
 * const transport = new WebSocketTransport("ws://localhost:37593");
 * 
 * // Different sites - no localhost assumptions  
 * const transport = new WebSocketTransport("wss://widgets.continuum.ai:443");
 * 
 * // Multiple widget instances - factory resolves
 * const widget1 = factory.createTransport('websocket', { host: 'site1.com', port: 9001 });
 * const widget2 = factory.createTransport('websocket', { host: 'site2.com', port: 9002 });
 * 
 * // Protocol changes - same interface
 * const transport = factory.createTransport('http', { host: 'api.continuum.ai', path: '/messages' });
 * const transport = factory.createTransport('udp', { host: 'mesh.continuum.ai', port: 37472 });
 */