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

export abstract class TransportFactoryBase implements ITransportFactory {
  protected environment: JTAGContext['environment'];

  constructor(environment: JTAGContext['environment']) {
    this.environment = environment;
  }

  /**
   * Create appropriate transport for this environment
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
   * Create WebSocket transport - delegates to concrete implementation
   */
  async createWebSocketTransport(
    environment: JTAGContext['environment'],
    config: TransportConfig
  ): Promise<JTAGTransport> {
    
    // Shared validation
    if (config.protocol !== 'websocket') {
      throw new Error(`WebSocket transport requires protocol 'websocket', got '${config.protocol}'`);
    }

    console.log(`🔗 ${this.getFactoryLabel()}: Creating WebSocket ${config.role} transport`);
    
    // Delegate to environment-specific implementation
    return this.createWebSocketTransportImpl(environment, config);
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
   * Create standardized transport creation result
   */
  protected createTransportResult(transport: JTAGTransport, protocol: string): JTAGTransport {
    console.log(`✅ ${this.getFactoryLabel()}: ${protocol} transport created successfully`);
    return transport;
  }

  // Abstract methods that environments must implement
  protected abstract createTransportImpl(
    environment: JTAGContext['environment'], 
    config: TransportConfig
  ): Promise<JTAGTransport>;

  protected abstract createWebSocketTransportImpl(
    environment: JTAGContext['environment'],
    config: TransportConfig
  ): Promise<JTAGTransport>;

  protected abstract getFactoryLabel(): string;
}