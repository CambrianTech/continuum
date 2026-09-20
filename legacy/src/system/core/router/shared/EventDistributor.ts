/**
 * EventDistributor - Dedicated P2P event broadcasting and distribution
 * 
 * Handles fire-and-forget event distribution across:
 * - Local subscribers
 * - Remote P2P nodes  
 * - Browser clients
 * - Server instances
 * 
 * CLEAR SEPARATION: Events are fire-and-forget, Promises are request-response
 */

import type { JTAGMessage, JTAGContext } from '../../types/JTAGTypes';
import { JTAGMessageTypes } from '../../types/JTAGTypes';
import type { JTAGTransport } from '../../../transports';
import { TRANSPORT_TYPES } from '../../../transports';
import { RouterUtilities } from './RouterUtilities';
import { 
  REMOTE_ENDPOINT_PREFIX,
  COMMAND_TYPES 
} from './RouterConstants';
import type { EventResult } from './RouterTypes';

export interface IEventDistributor {
  distributeEvent(message: JTAGMessage): Promise<EventResult>;
  registerEventTransport(type: TRANSPORT_TYPES, transport: JTAGTransport): void;
  removeEventTransport(type: TRANSPORT_TYPES): void;
}

export interface EventDistributionTarget {
  readonly type: 'local' | 'remote' | 'p2p' | 'broadcast';
  readonly transportType?: TRANSPORT_TYPES;
  readonly nodeId?: string;
  readonly priority: 'high' | 'medium' | 'low';
}

/**
 * EventDistributor - Clean event broadcasting system
 * 
 * DESIGN PRINCIPLES:
 * - Events are fire-and-forget (no response expected)
 * - Support multiple distribution targets simultaneously
 * - P2P-aware for mesh networking
 * - Priority-based delivery for critical events
 */
export class EventDistributor implements IEventDistributor {
  private readonly context: JTAGContext;
  private readonly eventTransports = new Map<TRANSPORT_TYPES, JTAGTransport>();

  constructor(context: JTAGContext) {
    this.context = context;
  }

  /**
   * Distribute event to appropriate targets
   */
  async distributeEvent(message: JTAGMessage): Promise<EventResult> {
    if (!JTAGMessageTypes.isEvent(message)) {
      throw new Error('EventDistributor: Only event messages are supported');
    }

    // Determine distribution targets
    const targets = this.determineDistributionTargets(message);
    const results: EventResult[] = [];

    // Distribute to all targets
    for (const target of targets) {
      try {
        const result = await this.distributeToTarget(message, target);
        results.push(result);
      } catch (error) {
        console.warn(`⚠️ EventDistributor: Failed to distribute to ${target.type}:`, error);
        results.push({ success: false, delivered: false, error: error instanceof Error ? error.message : 'Distribution failed' });
      }
    }

    // Aggregate results
    return this.aggregateResults(results);
  }

  /**
   * Register transport for event distribution
   */
  registerEventTransport(type: TRANSPORT_TYPES, transport: JTAGTransport): void {
    this.eventTransports.set(type, transport);
    console.log(`📡 EventDistributor: Registered ${type} transport for event distribution`);
  }

  /**
   * Remove event transport
   */
  removeEventTransport(type: TRANSPORT_TYPES): void {
    this.eventTransports.delete(type);
    console.log(`📡 EventDistributor: Removed ${type} transport`);
  }

  /**
   * Determine where this event should be distributed
   */
  private determineDistributionTargets(message: JTAGMessage): EventDistributionTarget[] {
    const targets: EventDistributionTarget[] = [];

    // Check for P2P remote targets
    if (message.endpoint.includes(REMOTE_ENDPOINT_PREFIX)) {
      const remoteInfo = RouterUtilities.parseRemoteEndpoint(message.endpoint);
      if (remoteInfo) {
        targets.push({
          type: 'p2p',
          transportType: TRANSPORT_TYPES.P2P,
          nodeId: remoteInfo.nodeId,
          priority: this.determineEventPriority(message)
        });
      }
    }

    // Check for cross-context distribution (browser <-> server)
    const targetEnvironment = RouterUtilities.extractEnvironment(message.endpoint, this.context.environment);
    if (targetEnvironment !== this.context.environment) {
      targets.push({
        type: 'remote',
        transportType: TRANSPORT_TYPES.CROSS_CONTEXT,
        priority: this.determineEventPriority(message)
      });
    }

    // Check for broadcast events (system-wide notifications)
    if (this.isBroadcastEvent(message)) {
      targets.push({
        type: 'broadcast',
        priority: 'high'
      });
    }

    // Default: local distribution
    if (targets.length === 0) {
      targets.push({
        type: 'local',
        priority: 'medium'
      });
    }

    return targets;
  }

  /**
   * Distribute event to specific target
   */
  private async distributeToTarget(message: JTAGMessage, target: EventDistributionTarget): Promise<EventResult> {
    console.log(`📢 EventDistributor: Distributing ${message.endpoint} to ${target.type} (${target.priority} priority)`);

    switch (target.type) {
      case 'p2p':
        return await this.distributeToP2P(message, target);
      
      case 'remote':
        return await this.distributeToRemote(message, target);
      
      case 'broadcast':
        return await this.distributeToBroadcast(message);
      
      case 'local':
        return { success: true, delivered: false, local: true };
      
      default:
        throw new Error(`Unknown distribution target: ${target.type}`);
    }
  }

  /**
   * Distribute to P2P network
   */
  private async distributeToP2P(message: JTAGMessage, target: EventDistributionTarget): Promise<EventResult> {
    const p2pTransport = this.eventTransports.get(TRANSPORT_TYPES.P2P);
    
    if (!p2pTransport) {
      throw new Error('P2P transport not available for event distribution');
    }

    // Create P2P message (strip remote prefix)
    const remoteInfo = RouterUtilities.parseRemoteEndpoint(message.endpoint);
    if (!remoteInfo) {
      throw new Error('Invalid P2P endpoint format');
    }

    const p2pMessage: JTAGMessage = {
      ...message,
      endpoint: remoteInfo.targetPath
    };

    await p2pTransport.send(p2pMessage);
    
    return { 
      success: true, 
      delivered: true, 
      p2pTarget: target.nodeId 
    };
  }

  /**
   * Distribute to remote context (browser <-> server)
   */
  private async distributeToRemote(message: JTAGMessage, target: EventDistributionTarget): Promise<EventResult> {
    if (!target.transportType) {
      throw new Error('Transport type required for remote distribution');
    }

    const transport = this.eventTransports.get(target.transportType);
    if (!transport) {
      throw new Error(`Transport ${target.transportType} not available for remote distribution`);
    }

    await transport.send(message);
    
    return { 
      success: true, 
      delivered: true 
    };
  }

  /**
   * Distribute to all interested parties (broadcast)
   */
  private async distributeToBroadcast(message: JTAGMessage): Promise<EventResult> {
    const results: EventResult[] = [];

    // Send to all available transports for maximum reach
    for (const [type, transport] of this.eventTransports) {
      try {
        await transport.send(message);
        results.push({ success: true, delivered: true });
      } catch (error) {
        console.warn(`⚠️ EventDistributor: Broadcast failed for ${type}:`, error);
        results.push({ success: false, delivered: false });
      }
    }

    return this.aggregateResults(results);
  }

  /**
   * Determine event priority based on content
   */
  private determineEventPriority(message: JTAGMessage): 'high' | 'medium' | 'low' {
    // Critical system events get high priority
    if (message.endpoint.includes('health') || 
        message.endpoint.includes('error') ||
        message.endpoint.includes('shutdown')) {
      return 'high';
    }

    // Console messages get low priority to avoid flooding
    if (message.endpoint.includes(COMMAND_TYPES.CONSOLE)) {
      return 'low';
    }

    // Default medium priority
    return 'medium';
  }

  /**
   * Check if event should be broadcast system-wide
   */
  private isBroadcastEvent(message: JTAGMessage): boolean {
    // System notifications, health alerts, shutdown events
    return message.endpoint.includes('system') ||
           message.endpoint.includes('shutdown') ||
           message.endpoint.includes('alert');
  }

  /**
   * Aggregate multiple distribution results
   */
  private aggregateResults(results: EventResult[]): EventResult {
    if (results.length === 0) {
      return { success: false, delivered: false, error: 'No distribution targets' };
    }

    const successCount = results.filter(r => r.success).length;
    const deliveredCount = results.filter(r => r.delivered).length;

    return {
      success: successCount > 0,
      delivered: deliveredCount > 0,
      distributionTargets: results.length,
      successfulTargets: successCount
    };
  }
}