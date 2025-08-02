/**
 * JTAGRouterBase - Base Router with Abstract Transport Factory
 * 
 * Minimal base class that only abstracts transport factory selection.
 * All routing logic stays in concrete implementations.
 */

import { JTAGModule } from '../../shared/JTAGModule';
import type { JTAGContext, JTAGMessage, JTAGEnvironment } from '../../types/JTAGTypes';
import type { ITransportFactory } from '../../../transports';
import type { JTAGRouterConfig } from './JTAGRouterTypes';
import type { JTAGResponsePayload } from '../../types/ResponseTypes';
import { EndpointMatcher } from './EndpointMatcher';

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

  /**
   * Extract environment from endpoint path (moved from JTAGRouter)
   */
  protected extractEnvironment(endpoint: string): JTAGEnvironment {
    if (endpoint.startsWith('browser/')) return 'browser';
    if (endpoint.startsWith('server/')) return 'server';
    if (endpoint.startsWith('remote/')) return 'remote';
    
    return this.context.environment;
  }

  /**
   * Parse remote endpoint for P2P routing (moved from JTAGRouter)
   * Format: /remote/{nodeId}/daemon/command or /remote/{nodeId}/server/daemon/command
   */
  protected parseRemoteEndpoint(endpoint: string): { nodeId: string; targetPath: string } | null {
    if (!endpoint.startsWith('remote/')) {
      return null;
    }

    const parts = endpoint.split('/');
    if (parts.length < 3) {
      return null;
    }

    const nodeId = parts[1]; // remote/{nodeId}/...
    const targetPath = parts.slice(2).join('/'); // everything after nodeId

    return { nodeId, targetPath };
  }

  /**
   * Determine sender environment for response routing (moved from JTAGRouter)
   */
  protected determineSenderEnvironment(message: JTAGMessage): JTAGEnvironment {
    // If origin is 'client', they came from a transport connection  
    if (message.origin === 'client') {
      // The key insight: responses to transport clients should stay in the same environment
      // The transport layer will handle routing back to the actual client
      console.log(`🎯 ${this.toString()}: Transport client detected - keeping response in ${this.context.environment} environment`);
      return this.context.environment;
    }
    
    // For other origins, extract environment from the origin path
    return this.extractEnvironment(message.origin);
  }

  /**
   * Get environment-specific transport factory
   */
  protected abstract getTransportFactory(): Promise<ITransportFactory>;
}