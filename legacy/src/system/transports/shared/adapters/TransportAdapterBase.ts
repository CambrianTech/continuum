/**
 * Transport Adapter Base - Generic adapter foundation with separated concerns
 * 
 * Provides generic base functionality for all transport adapters.
 * Each transport protocol gets its own adapter that extends this base.
 * 
 * ARCHITECTURE PRINCIPLES:
 * - Well-separated concerns per class
 * - Generic whenever possible
 * - Strong TypeScript typing
 * - Adapter pattern for protocol-specific implementations
 */

import type {
  TransportProtocol,
  TransportRole,
  EnvironmentType,
  TransportProtocolRegistry,
  CrossEnvironmentTransport,
  TransportSendResult,
  TransportError
} from '../TransportProtocolContracts';

/**
 * Generic Transport Adapter Configuration
 * Base configuration that all adapters extend
 */
export interface TransportAdapterConfig<T extends TransportProtocol> {
  readonly protocol: T;
  readonly role: TransportRole;
  readonly environment: EnvironmentType;
  readonly name?: string;
  
  // Protocol-specific configuration (strongly typed)
  protocolConfig: TransportProtocolRegistry[T]['config'];
  
  // Generic adapter options
  adapterOptions?: {
    readonly autoReconnect?: boolean;
    readonly maxRetries?: number;
    readonly retryDelay?: number;
    readonly healthCheckInterval?: number;
  };
}

/**
 * Generic Transport Adapter Base Class
 * 
 * Provides common functionality that all transport adapters need.
 * Specific protocols extend this and implement protocol-specific methods.
 */
export abstract class TransportAdapterBase<T extends TransportProtocol> implements CrossEnvironmentTransport<T> {
  public readonly name: string;
  public readonly protocol: T;
  public readonly environment: EnvironmentType;
  public readonly role: TransportRole;
  
  protected readonly config: TransportAdapterConfig<T>;
  protected connected = false;
  
  // Generic callback management - separated concern
  private connectCallbacks = new Set<() => void>();
  private disconnectCallbacks = new Set<(reason?: string) => void>();
  private dataCallbacks = new Set<(data: TransportProtocolRegistry[T]['dataFormat']) => void>();
  private errorCallbacks = new Set<(error: TransportError) => void>();
  
  constructor(config: TransportAdapterConfig<T>) {
    this.config = config;
    this.protocol = config.protocol;
    this.role = config.role;
    this.environment = config.environment;
    this.name = config.name || `${config.protocol}-${config.role}-${config.environment}`;
  }
  
  /**
   * Connection management - generic implementation
   */
  public isConnected(): boolean {
    return this.connected;
  }
  
  /**
   * Event system - generic callback management with separated concerns
   */
  public onConnect(callback: () => void): void {
    this.connectCallbacks.add(callback);
  }
  
  public onDisconnect(callback: (reason?: string) => void): void {
    this.disconnectCallbacks.add(callback);
  }
  
  public onData(callback: (data: TransportProtocolRegistry[T]['dataFormat']) => void): void {
    this.dataCallbacks.add(callback);
  }
  
  public onError(callback: (error: TransportError) => void): void {
    this.errorCallbacks.add(callback);
  }
  
  /**
   * Protected event emitters - for use by adapter implementations
   */
  protected emitConnect(): void {
    this.connected = true;
    this.connectCallbacks.forEach(callback => {
      try {
        callback();
      } catch (error) {
        console.error(`Connect callback error in ${this.name}:`, error);
      }
    });
  }
  
  protected emitDisconnect(reason?: string): void {
    this.connected = false;
    this.disconnectCallbacks.forEach(callback => {
      try {
        callback(reason);
      } catch (error) {
        console.error(`Disconnect callback error in ${this.name}:`, error);
      }
    });
  }
  
  protected emitData(data: TransportProtocolRegistry[T]['dataFormat']): void {
    this.dataCallbacks.forEach(callback => {
      try {
        callback(data);
      } catch (error) {
        console.error(`Data callback error in ${this.name}:`, error);
      }
    });
  }
  
  protected emitError(error: TransportError): void {
    this.errorCallbacks.forEach(callback => {
      try {
        callback(error);
      } catch (error: unknown) {
        console.error(`Error callback error in ${this.name}:`, error);
      }
    });
  }
  
  /**
   * Generic error creation helper
   */
  protected createTransportError(
    message: string,
    code: TransportError['code'],
    originalError?: Error
  ): TransportError {
    const error = new Error(message) as TransportError;
    error.code = code;
    error.protocol = this.protocol;
    error.timestamp = new Date().toISOString();
    error.metadata = {
      transportName: this.name,
      role: this.role,
      environment: this.environment,
      originalError: originalError?.message
    };
    return error;
  }
  
  /**
   * Generic send result helper
   */
  protected createSendResult(
    success: boolean,
    bytesTransmitted?: number,
    metadata?: Record<string, unknown>
  ): TransportSendResult {
    return {
      success,
      timestamp: new Date().toISOString(),
      bytesTransmitted,
      metadata: metadata ? Object.freeze(metadata) : undefined
    };
  }
  
  /**
   * Abstract methods - each adapter implements protocol-specific behavior
   */
  public abstract connect(config?: TransportProtocolRegistry[T]['config']): Promise<void>;
  public abstract send(data: TransportProtocolRegistry[T]['dataFormat']): Promise<TransportSendResult>;
  public abstract disconnect(): Promise<void>;
  
  /**
   * Generic cleanup - called by implementations
   */
  protected cleanup(): void {
    this.connectCallbacks.clear();
    this.disconnectCallbacks.clear();
    this.dataCallbacks.clear();
    this.errorCallbacks.clear();
    this.connected = false;
  }
}