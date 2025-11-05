/**
 * Transport Protocol Contracts - Strongly typed cross-environment API
 * 
 * This defines our module's local API with strict TypeScript enforcement.
 * Critical for network communication across browser/server environments.
 * 
 * PRINCIPLE: Define interfaces in advance, enforce with TypeScript, share across environments
 */

/**
 * Supported transport protocols - TypeScript const assertion for strict typing
 */
export const TRANSPORT_PROTOCOLS = {
  WEBSOCKET: 'websocket',
  HTTP: 'http',
  UDP_MULTICAST: 'udp-multicast'
} as const;

export type TransportProtocol = typeof TRANSPORT_PROTOCOLS[keyof typeof TRANSPORT_PROTOCOLS];

/**
 * Transport roles - TypeScript enforced network behavior contracts
 */
export const TRANSPORT_ROLES = {
  CLIENT: 'client',
  SERVER: 'server',
  PEER: 'peer'
} as const;

export type TransportRole = typeof TRANSPORT_ROLES[keyof typeof TRANSPORT_ROLES];

/**
 * Environment types - TypeScript enforced execution context
 */
export const ENVIRONMENT_TYPES = {
  BROWSER: 'browser',
  SERVER: 'server',
  SHARED: 'shared'
} as const;

export type EnvironmentType = typeof ENVIRONMENT_TYPES[keyof typeof ENVIRONMENT_TYPES];

/**
 * WebSocket Protocol Contract - Cross-environment API
 */
export interface WebSocketProtocolContract {
  readonly protocol: typeof TRANSPORT_PROTOCOLS.WEBSOCKET;
  readonly supportedRoles: readonly [typeof TRANSPORT_ROLES.CLIENT, typeof TRANSPORT_ROLES.SERVER];
  readonly supportedEnvironments: readonly [typeof ENVIRONMENT_TYPES.BROWSER, typeof ENVIRONMENT_TYPES.SERVER];
  
  // Required configuration parameters - TypeScript enforced
  config: {
    url?: string;
    host?: string;
    port?: number;
    reconnectAttempts?: number;
    reconnectDelay?: number;
    pingInterval?: number;
  };
  
  // Data format contract
  dataFormat: string | Uint8Array;
  
  // Connection lifecycle contract
  connectionStates: 'connecting' | 'connected' | 'disconnecting' | 'disconnected';
}

/**
 * HTTP Protocol Contract - Cross-environment API  
 */
export interface HTTPProtocolContract {
  readonly protocol: typeof TRANSPORT_PROTOCOLS.HTTP;
  readonly supportedRoles: readonly [typeof TRANSPORT_ROLES.CLIENT];
  readonly supportedEnvironments: readonly [typeof ENVIRONMENT_TYPES.BROWSER, typeof ENVIRONMENT_TYPES.SERVER];
  
  // Required configuration parameters
  config: {
    baseUrl: string;
    timeout?: number;
    retryAttempts?: number;
    headers?: Record<string, string>;
  };
  
  // Data format contract  
  dataFormat: string;
  
  // HTTP-specific contract
  methods: 'GET' | 'POST' | 'PUT' | 'DELETE';
}

/**
 * UDP Multicast Protocol Contract - Cross-environment API
 */
export interface UDPMulticastProtocolContract {
  readonly protocol: typeof TRANSPORT_PROTOCOLS.UDP_MULTICAST;
  readonly supportedRoles: readonly [typeof TRANSPORT_ROLES.PEER];
  readonly supportedEnvironments: readonly [typeof ENVIRONMENT_TYPES.SERVER]; // Node.js only
  
  // Required configuration parameters
  config: {
    multicastAddress: string;
    multicastPort: number;
    unicastPort?: number;
    nodeId?: string;
    capabilities?: readonly string[];
  };
  
  // Data format contract
  dataFormat: Uint8Array;
  
  // P2P-specific contract
  nodeTypes: 'server' | 'browser' | 'mobile' | 'ai-agent';
}

/**
 * Transport Protocol Registry - TypeScript enforced protocol lookup
 * 
 * This creates a strongly typed registry that prevents protocol mismatches
 * across environments and ensures API contract compliance.
 */
export interface TransportProtocolRegistry {
  [TRANSPORT_PROTOCOLS.WEBSOCKET]: WebSocketProtocolContract;
  [TRANSPORT_PROTOCOLS.HTTP]: HTTPProtocolContract;
  [TRANSPORT_PROTOCOLS.UDP_MULTICAST]: UDPMulticastProtocolContract;
}

/**
 * Cross-Environment Transport Factory Contract
 * 
 * Defines the API that both browser and server implementations must follow
 */
export interface CrossEnvironmentTransportFactory<T extends TransportProtocol> {
  readonly protocol: T;
  readonly environment: EnvironmentType;
  
  // TypeScript enforced creation contract
  create(config: TransportProtocolRegistry[T]['config']): Promise<CrossEnvironmentTransport<T>>;
  
  // Protocol support validation
  supports(protocol: TransportProtocol, role: TransportRole): protocol is T;
  
  // Environment compatibility check
  isCompatible(environment: EnvironmentType): boolean;
}

/**
 * Cross-Environment Transport Interface Contract
 * 
 * The core API that all transport implementations must provide
 */
export interface CrossEnvironmentTransport<T extends TransportProtocol> {
  readonly name: string;
  readonly protocol: T;
  readonly environment: EnvironmentType;
  readonly role: TransportRole;
  
  // Core transport operations - strongly typed
  connect(config?: TransportProtocolRegistry[T]['config']): Promise<void>;
  send(data: TransportProtocolRegistry[T]['dataFormat']): Promise<TransportSendResult>;
  isConnected(): boolean;
  disconnect(): Promise<void>;
  
  // Event system - typed callbacks
  onConnect(callback: () => void): void;
  onDisconnect(callback: (reason?: string) => void): void;
  onData(callback: (data: TransportProtocolRegistry[T]['dataFormat']) => void): void;
  onError(callback: (error: TransportError) => void): void;
}

/**
 * Strongly typed transport send result
 */
export interface TransportSendResult {
  readonly success: boolean;
  readonly timestamp: string;
  readonly bytesTransmitted?: number;
  readonly metadata?: Readonly<Record<string, unknown>>;
}

/**
 * Strongly typed transport error
 */
export interface TransportError extends Error {
  readonly code: 'CONNECTION_FAILED' | 'SEND_FAILED' | 'PROTOCOL_ERROR' | 'TIMEOUT' | 'UNKNOWN';
  readonly protocol: TransportProtocol;
  readonly timestamp: string;
  readonly metadata?: Readonly<Record<string, unknown>>;
}

/**
 * Type guards for protocol validation - runtime TypeScript enforcement
 */
export const isWebSocketProtocol = (protocol: string): protocol is typeof TRANSPORT_PROTOCOLS.WEBSOCKET => {
  return protocol === TRANSPORT_PROTOCOLS.WEBSOCKET;
};

export const isHTTPProtocol = (protocol: string): protocol is typeof TRANSPORT_PROTOCOLS.HTTP => {
  return protocol === TRANSPORT_PROTOCOLS.HTTP;
};

export const isUDPMulticastProtocol = (protocol: string): protocol is typeof TRANSPORT_PROTOCOLS.UDP_MULTICAST => {
  return protocol === TRANSPORT_PROTOCOLS.UDP_MULTICAST;
};

export const isValidTransportProtocol = (protocol: string): protocol is TransportProtocol => {
  return isWebSocketProtocol(protocol) || isHTTPProtocol(protocol) || isUDPMulticastProtocol(protocol);
};

/**
 * Type guard for role validation
 */
export const isValidTransportRole = (role: string): role is TransportRole => {
  return Object.values(TRANSPORT_ROLES).includes(role as TransportRole);
};

/**
 * Type guard for environment validation
 */
export const isValidEnvironment = (env: string): env is EnvironmentType => {
  return Object.values(ENVIRONMENT_TYPES).includes(env as EnvironmentType);
};