/**
 * JTAGRouterBase - Base Router with Abstract Transport Factory
 * 
 * Minimal base class that only abstracts transport factory selection.
 * All routing logic stays in concrete implementations.
 */

import { JTAGModule } from '../../shared/JTAGModule';
import type { JTAGContext, JTAGMessage, JTAGEnvironment } from '../../types/JTAGTypes';
import type { ITransportFactory, JTAGTransport, TransportConfig } from '../../../transports';
import { TRANSPORT_TYPES } from '../../../transports';
import type { JTAGRouterConfig } from './JTAGRouterTypes';
import type { JTAGResponsePayload } from '../../types/ResponseTypes';
import { EndpointMatcher } from './EndpointMatcher';
import { RouterUtilities } from './RouterUtilities';

/**
 * Message Subscriber Interface - Core contract for message handling
 */
export interface MessageSubscriber {
  handleMessage(message: JTAGMessage): Promise<JTAGResponsePayload>;
  get endpoint(): string;
  get uuid(): string;
}

export abstract class JTAGRouterBase extends JTAGModule {
  
  // Core subscriber management (moved from JTAGRouter)  
  protected readonly endpointMatcher = new EndpointMatcher<MessageSubscriber>();
  
  // Transport management pattern (moved from JTAGRouter)
  protected readonly transports = new Map<TRANSPORT_TYPES, JTAGTransport>();
  protected isInitialized = false;
  
  constructor(name: string, context: JTAGContext, config: JTAGRouterConfig = {}) {
    super(name, context);
  }

  /**
   * Register subscriber for message routing (moved from JTAGRouter)
   */
  registerSubscriber(endpoint: string, subscriber: MessageSubscriber): void {
    const fullEndpoint = `${this.context.environment}/${endpoint}`;
    this.endpointMatcher.register(fullEndpoint, subscriber);
    
    // Only register short endpoint if it doesn't already exist to avoid duplicates
    if (!this.endpointMatcher.hasExact(endpoint)) {
      this.endpointMatcher.register(endpoint, subscriber);
      console.log(`📋 ${this.toString()}: Registered subscriber at ${fullEndpoint} AND ${endpoint}`);
    } else {
      console.log(`📋 ${this.toString()}: Registered subscriber at ${fullEndpoint} (${endpoint} already exists)`);
    }
  }

  // Pure utility methods moved to RouterUtilities class
  // Object-specific methods that need 'this' context remain here

  /**
   * Get environment-specific transport factory
   */
  protected abstract getTransportFactory(): Promise<ITransportFactory>;

  /**
   * Get transport status - default implementation using shared transport map
   */
  getTransportStatus(): { 
    initialized: boolean; 
    transportCount: number; 
    transports: Array<{ name: string; connected: boolean; type: string; }> 
  } {
    return {
      initialized: this.isInitialized,
      transportCount: this.transports.size,
      transports: Array.from(this.transports.entries()).map(([type, transport]) => ({
        name: transport.name,
        connected: transport.isConnected(),
        type: type.toString()
      }))
    };
  }

  /**
   * Initialize the router - to be implemented by concrete routers
   */
  abstract initialize(): Promise<void>;

  /**
   * Shutdown the router - to be implemented by concrete routers  
   */
  abstract shutdown(): Promise<void>;
}