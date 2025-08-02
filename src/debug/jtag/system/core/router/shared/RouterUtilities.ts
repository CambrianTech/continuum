/**
 * RouterUtilities - Pure utility functions for message routing
 * 
 * PURPOSE: Static utilities that don't need object state
 * SEPARATION: Keep object-specific methods in JTAGRouterBase
 * REUSABILITY: Can be used by any router implementation
 */

import type { JTAGMessage, JTAGEnvironment } from '../../types/JTAGTypes';
import { MessagePriority } from './queuing/JTAGMessageQueue';
import { DefaultPriorityStrategy, type IMessagePriorityStrategy } from './priority/MessagePriorityStrategy';

/**
 * RouterUtilities - Static utility methods for message processing
 */
export class RouterUtilities {
  
  private static priorityStrategy: IMessagePriorityStrategy = new DefaultPriorityStrategy();
  
  /**
   * Extract environment from endpoint path
   */
  static extractEnvironment(endpoint: string, fallbackEnvironment: JTAGEnvironment): JTAGEnvironment {
    if (endpoint.startsWith('browser/')) return 'browser';
    if (endpoint.startsWith('server/')) return 'server';
    if (endpoint.startsWith('remote/')) return 'remote';
    
    return fallbackEnvironment;
  }

  /**
   * Parse remote endpoint for P2P routing
   * Format: /remote/{nodeId}/daemon/command or /remote/{nodeId}/server/daemon/command
   */
  static parseRemoteEndpoint(endpoint: string): { nodeId: string; targetPath: string } | null {
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
   * Determine message priority using pluggable strategy
   */
  static determinePriority(message: JTAGMessage): MessagePriority {
    return RouterUtilities.priorityStrategy.determinePriority(message);
  }

  /**
   * Set custom priority strategy - allows pluggable priority rules
   */
  static setPriorityStrategy(strategy: IMessagePriorityStrategy): void {
    RouterUtilities.priorityStrategy = strategy;
  }

  /**
   * Get current priority strategy
   */
  static getPriorityStrategy(): IMessagePriorityStrategy {
    return RouterUtilities.priorityStrategy;
  }

  /**
   * Determine sender environment for response routing
   */
  static determineSenderEnvironment(message: JTAGMessage, currentEnvironment: JTAGEnvironment): JTAGEnvironment {
    // If origin is 'client', they came from a transport connection  
    if (message.origin === 'client') {
      // The key insight: responses to transport clients should stay in the same environment
      // The transport layer will handle routing back to the actual client
      console.log(`🎯 RouterUtilities: Transport client detected - keeping response in ${currentEnvironment} environment`);
      return currentEnvironment;
    }
    
    // For other origins, extract environment from the origin path
    return RouterUtilities.extractEnvironment(message.origin, currentEnvironment);
  }
}