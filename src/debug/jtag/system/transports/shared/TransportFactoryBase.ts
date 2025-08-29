/**
 * Transport Factory Base - Shared base class for all transport factory implementations
 * 
 * Follows the Universal Module Architecture pattern:
 * - 85% of factory logic in shared base class
 * - 15% environment-specific implementation in browser/server
 * 
 * Provides common functionality like validation, error handling, and adapter creation patterns.
 */

import type { JTAGContext } from '../../core/types/JTAGTypes';
import type { JTAGTransport, TransportConfig } from './TransportTypes';
import { TransportConfigHelper } from './TransportConfig';
import type { ITransportFactory } from './ITransportFactory';
import type { 
  PureTransport, 
  PureTransportFactory, 
  TransportRequest,
  PureTransportConfig
} from './PureTransportTypes';

/**
 * Legacy Transport Bridge Interface - Type-safe bridge to legacy transports
 * Eliminates `any` usage while we migrate to pure transports
 */
interface LegacyTransportBridge {
  sendMessage?: (data: string | Uint8Array) => Promise<void>;
  isConnected?: () => boolean;
  disconnect?: () => Promise<void>;
}

/**
 * Strong-typed protocol validator and converter
 * Ensures only valid protocols are used throughout the system
 */
export const validateAndConvertProtocol = (protocol: string): TransportConfig['protocol'] => {
  const validProtocols = ['websocket', 'http', 'udp-multicast'] as const;
  if (validProtocols.includes(protocol as any)) {
    return protocol as TransportConfig['protocol'];
  }
  throw new Error(`Invalid protocol: ${protocol}. Valid protocols: ${validProtocols.join(', ')}`);
};

export abstract class TransportFactoryBase implements ITransportFactory, PureTransportFactory {
  protected environment: JTAGContext['environment'];

  constructor(environment: JTAGContext['environment']) {
    this.environment = environment;
  }

  /**
   * Create appropriate transport for this environment (LEGACY)
   * Template method pattern - delegates to concrete implementations
   */
  async createTransport(
    environment: JTAGContext['environment'], 
    config: TransportConfig
  ): Promise<JTAGTransport> {
    
    // Shared validation logic
    TransportConfigHelper.validateConfig(config);
    
    console.log(`🏭 ${this.getFactoryLabel()}: Creating ${config.protocol} transport for ${environment} environment`);
    
    // Delegate to environment-specific implementation
    return this.createTransportImpl(environment, config);
  }

  /**
   * Create WebSocket transport - DEPRECATED: Use registry-based createTransport() instead
   */
  async createWebSocketTransport(
    environment: JTAGContext['environment'],
    config: TransportConfig
  ): Promise<JTAGTransport> {
    
    console.log(`🔗 ${this.getFactoryLabel()}: DEPRECATED WebSocket method - delegating to registry-based creation`);
    
    // Delegate to registry-based implementation
    return this.createTransport(environment, config);
  }

  /**
   * Shared error handling for unsupported protocols
   */
  protected throwUnsupportedProtocol(protocol: string): never {
    throw new Error(`Unsupported transport protocol: ${protocol} in ${this.environment} environment`);
  }

  /**
   * Shared validation for transport roles
   */
  protected validateRole(role: string, supportedRoles: string[]): void {
    if (!supportedRoles.includes(role)) {
      throw new Error(`Transport role '${role}' not supported in ${this.environment} environment. Supported: ${supportedRoles.join(', ')}`);
    }
  }

  /**
   * PURE TRANSPORT FACTORY INTERFACE IMPLEMENTATION
   * New dumb pipe architecture - Configuration to Destination Resolver
   */

  /**
   * Legacy interface support for backward compatibility
   */
  async create(config: PureTransportConfig): Promise<PureTransport> {
    // Convert PureTransportConfig to TransportRequest for new architecture
    const request: TransportRequest = {
      protocol: config.protocol,
      role: 'client', // Default assumption for legacy compatibility
      host: config.host,
      port: config.port,
      environment: this.environment === 'remote' ? 'server' : this.environment as 'browser' | 'server',
      configSource: config
    };
    
    return this.createPureTransport(config.protocol, request);
  }

  /**
   * Check if protocol is supported by this factory
   */
  supports(protocol: string): boolean {
    return this.getSupportedProtocols().includes(protocol);
  }

  /**
   * New dumb pipe interface - Create transport with resolved destination
   * This is the core method that replaces hardcoded configuration assumptions
   */
  async createPureTransport(protocol: string, request: TransportRequest): Promise<PureTransport> {
    // Validate protocol support
    if (!this.supports(protocol)) {
      this.throwUnsupportedProtocol(protocol);
    }
    
    // Resolve destination from configuration
    const destination = this.resolveDestination(protocol, request);
    console.log(`🏭 ${this.getFactoryLabel()}: Creating ${protocol} transport for destination: ${destination}`);
    
    // Delegate to environment-specific implementation with resolved destination
    return this.createTransportWithDestination(protocol, request, destination);
  }

  /**
   * Resolve destination from configuration - eliminates hardcoded assumptions
   * This method interprets various configuration sources and produces concrete destinations
   */
  resolveDestination(protocol: string, request: TransportRequest): string {
    // Priority order: explicit request params > environment config > defaults
    const host = request.host ?? this.getDefaultHost(request.environment);
    const port = request.port ?? this.getDefaultPort(protocol, request.environment);
    const secure = request.secure ?? this.shouldUseSecure(request.environment);
    
    switch (protocol) {
      case 'websocket':
        const wsProtocol = secure ? 'wss' : 'ws';
        return `${wsProtocol}://${host}:${port}`;
        
      case 'http':
        const httpProtocol = secure ? 'https' : 'http';
        const path = request.path ?? '/';
        return `${httpProtocol}://${host}:${port}${path}`;
        
      case 'udp-multicast':
        // UDP uses different addressing scheme
        return `udp://${host}:${port}`;
        
      default:
        throw new Error(`Cannot resolve destination for unsupported protocol: ${protocol}`);
    }
  }

  /**
   * Create standardized transport creation result
   */
  protected createTransportResult(transport: JTAGTransport, protocol: string): JTAGTransport {
    console.log(`✅ ${this.getFactoryLabel()}: ${protocol} transport created successfully`);
    return transport;
  }

  /**
   * Get supported protocols list - environment implementations override this
   */
  getSupportedProtocols(): string[] {
    return ['websocket', 'http']; // Default protocols, environments can extend
  }

  /**
   * Get default host for environment - eliminates localhost hardcoding
   */
  protected getDefaultHost(environment: string): string {
    // Environment-specific implementations should override this
    // This default is only a fallback
    return 'localhost';
  }

  /**
   * Get default port for protocol and environment - eliminates port hardcoding
   */
  protected getDefaultPort(protocol: string, environment: string): number {
    // Environment-specific implementations should override this with dynamic port resolution
    // This is just a fallback for the base class
    switch (protocol) {
      case 'websocket': return 9001;
      case 'http': return 9002;
      case 'udp-multicast': return 37472;
      default: return 8000;
    }
  }

  /**
   * Determine if secure connection should be used - environment aware
   */
  protected shouldUseSecure(environment: string): boolean {
    // Environment-specific implementations should override this
    // Server might check for certificates, browser might check protocol
    return false; // Default to non-secure for local development
  }

  // Abstract methods that environments must implement

  /**
   * Legacy transport creation - delegates to new architecture
   */
  protected abstract createTransportImpl(
    environment: JTAGContext['environment'], 
    config: TransportConfig
  ): Promise<JTAGTransport>;

  /**
   * New architecture transport creation with resolved destination
   */
  protected abstract createTransportWithDestination(
    protocol: string,
    request: TransportRequest,
    destination: string
  ): Promise<PureTransport>;

  protected abstract getFactoryLabel(): string;
}