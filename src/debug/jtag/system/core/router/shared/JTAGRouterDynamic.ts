/**
 * JTAGRouterDynamic - Subclass Override for Dynamic Transport Management
 * 
 * STRATEGY: Extend existing JTAGRouter, override only the hardcoded transport parts
 * PRESERVE: All working promise correlation, message routing, health management
 * EVOLVE: Transport management from hardcoded enum to dynamic channels
 * 
 * INHERITANCE APPROACH:
 * - Keep parent's: ResponseCorrelator, message routing, health monitoring
 * - Override: Transport initialization, transport access, P2P routing
 * - Add: Dynamic transport discovery, unlimited channel types
 * 
 * MIGRATION PATH:
 * 1. Test JTAGRouterDynamic as drop-in replacement
 * 2. Verify screenshot → file/save promise chain works unchanged
 * 3. Enable P2P mesh capabilities
 * 4. Eventually replace original if desired
 */

import { JTAGRouter } from './JTAGRouter';
import type { JTAGContext, JTAGMessage } from '../../types/JTAGTypes';
import type { JTAGRouterConfig } from './JTAGRouterTypes';
import type { ITransportFactory, TransportConfig, JTAGTransport } from '../../../transports/shared/TransportTypes';
import type { 
  IChannelManager,
  ChannelId,
  ChannelConfig,
  ActiveChannel,
  PROTOCOLS,
  ENVIRONMENTS,
  TypedEndpoint,
  RoutingContext
} from '../../channels/shared/ChannelTypes';
import { ChannelManager } from '../../channels/shared/ChannelManager';
import type { RouterResult } from './RouterTypes';
import { TRANSPORT_TYPES } from '../../../transports';

// ============================================================================
// DYNAMIC TRANSPORT REGISTRY - REPLACES HARDCODED ENUM MAP
// ============================================================================

/**
 * Dynamic Transport Registry - Modern replacement for Map<TRANSPORT_TYPES, JTAGTransport>
 */
class DynamicTransportRegistry {
  private channels = new Map<string, ActiveChannel>();
  private purposes = new Map<ActiveChannel, string>();

  /**
   * Register channel by purpose
   */
  register(purpose: string, channel: ActiveChannel): void {
    this.channels.set(purpose, channel);
    this.purposes.set(channel, purpose);
    console.log(`📋 DynamicRegistry: Registered ${purpose} channel via ${channel.id.protocol}`);
  }

  /**
   * Get transport for purpose (replaces enum-based lookup)
   */
  getTransport(purpose: string): JTAGTransport | undefined {
    const channel = this.channels.get(purpose);
    return channel?.transport;
  }

  /**
   * Get channel for purpose
   */
  getChannel(purpose: string): ActiveChannel | undefined {
    return this.channels.get(purpose);
  }

  /**
   * Check if transport exists and is connected
   */
  isAvailable(purpose: string): boolean {
    const channel = this.channels.get(purpose);
    return channel?.isConnected() ?? false;
  }

  /**
   * Get all registered purposes
   */
  getPurposes(): string[] {
    return Array.from(this.channels.keys());
  }

  /**
   * Get transport statistics
   */
  getStats() {
    const stats: Record<string, { connected: boolean; protocol: string }> = {};
    for (const [purpose, channel] of this.channels) {
      stats[purpose] = {
        connected: channel.isConnected(),
        protocol: channel.id.protocol
      };
    }
    return stats;
  }

  /**
   * Clear all transports
   */
  clear(): void {
    this.channels.clear();
    this.purposes.clear();
  }

  /**
   * Compatibility with parent's transport map
   */
  asTransportMap(): Map<string, JTAGTransport> {
    const map = new Map<string, JTAGTransport>();
    for (const [purpose, channel] of this.channels) {
      map.set(purpose, channel.transport);
    }
    return map;
  }
}

// ============================================================================
// DYNAMIC ROUTER SUBCLASS - OVERRIDES HARDCODED TRANSPORT MANAGEMENT
// ============================================================================

export class JTAGRouterDynamic extends JTAGRouter {
  private dynamicRegistry = new DynamicTransportRegistry();
  private channelManager!: IChannelManager;
  private transportFactory!: ITransportFactory;

  constructor(context: JTAGContext, config: JTAGRouterConfig = {}) {
    super(context, config);
    console.log(`🔧 JTAGRouterDynamic[${context.environment}]: Initialized with dynamic transport management`);
  }

  /**
   * Override: Initialize transport with dynamic channel management
   * REPLACES: Parent's hardcoded transport creation
   */
  async initializeTransport(config?: Partial<TransportConfig>): Promise<void> {
    console.log(`🔗 ${this.toString()}: Initializing dynamic transport channels`);
    
    // Get transport factory (use parent's abstract method)
    this.transportFactory = await this.getTransportFactory();
    this.channelManager = new ChannelManager(this.context, this.transportFactory);

    // Create cross-context channel (replaces hardcoded CROSS_CONTEXT)
    await this.createCrossContextChannel();

    // Enable P2P if configured (replaces commented P2P section in parent)
    if (this.shouldEnableP2P()) {
      await this.createP2PChannel();
    }

    // Set up message handlers using dynamic registry
    await this.setupDynamicMessageHandlers();
  }

  /**
   * Create cross-context channel (browser ↔ server)
   */
  private async createCrossContextChannel(): Promise<void> {
    const channelId: ChannelId = {
      environment: this.context.environment === 'browser' ? ENVIRONMENTS.SERVER : ENVIRONMENTS.BROWSER,
      protocol: PROTOCOLS.WEBSOCKET,
      instanceId: crypto.randomUUID() as any
    };

    const channelConfig: ChannelConfig = {
      id: channelId,
      transport: {
        protocol: 'websocket',
        role: this.context.environment === 'browser' ? 'client' : 'server',
        eventSystem: this.eventManager.events,
        sessionId: this.context.uuid,
        serverPort: this.context.environment === 'server' ? 9001 : undefined,
        serverUrl: this.context.environment === 'browser' ? 'ws://localhost:9001' : undefined,
        fallback: true,
        handler: this // Use parent's ITransportHandler implementation
      },
      handler: this
    };

    const result = await this.channelManager.createChannel(channelConfig);
    
    if (result.success) {
      this.dynamicRegistry.register('cross-context', result.channel!);
      console.log(`✅ Created cross-context channel: ${this.context.environment} → ${channelId.environment}`);
    } else {
      throw new Error(`Failed to create cross-context channel: ${result.error?.message}`);
    }
  }

  /**
   * Create P2P UDP multicast channel (enables mesh networking)
   */
  private async createP2PChannel(): Promise<void> {
    const channelId: ChannelId = {
      environment: ENVIRONMENTS.REMOTE,
      protocol: PROTOCOLS.UDP_MULTICAST,
      instanceId: crypto.randomUUID() as any
    };

    const channelConfig: ChannelConfig = {
      id: channelId,
      transport: {
        protocol: 'udp-multicast',
        role: 'peer',
        eventSystem: this.eventManager.events,
        sessionId: this.context.uuid,
        p2p: {
          nodeId: this.context.uuid, // Use session UUID as node ID
          nodeType: this.context.environment === 'browser' ? 'browser' : 'server',
          capabilities: ['screenshot', 'file-ops', 'commands'],
          multicastAddress: '239.255.0.1',
          multicastPort: 9003,
          unicastPort: 9004
        },
        handler: this
      },
      handler: this
    };

    try {
      const result = await this.channelManager.createChannel(channelConfig);
      
      if (result.success) {
        this.dynamicRegistry.register('p2p', result.channel!);
        console.log(`✅ Created P2P UDP multicast channel for mesh networking`);
      } else {
        console.warn(`⚠️ P2P channel creation failed: ${result.error?.message}`);
      }
    } catch (error) {
      console.warn(`⚠️ P2P transport not available:`, error);
      // P2P is optional - don't fail initialization
    }
  }

  /**
   * Check if P2P should be enabled
   */
  private shouldEnableP2P(): boolean {
    // Enable P2P if explicitly configured, or by default for development
    return process.env.ENABLE_P2P === 'true' || process.env.NODE_ENV === 'development';
  }

  /**
   * Setup message handlers for dynamic channels
   */
  private async setupDynamicMessageHandlers(): Promise<void> {
    for (const purpose of this.dynamicRegistry.getPurposes()) {
      const channel = this.dynamicRegistry.getChannel(purpose);
      if (channel?.transport.setMessageHandler) {
        channel.transport.setMessageHandler((message: JTAGMessage) => {
          this.postMessage(message).catch(console.error);
        });
        console.log(`✅ Message handler ready: ${purpose} (${channel.id.protocol})`);
      }
    }
  }

  /**
   * Override: Get transport by purpose instead of enum
   * REPLACES: Parent's transports.get(TRANSPORT_TYPES.CROSS_CONTEXT)
   */
  protected getTransportByPurpose(purpose: 'cross-context' | 'p2p' | string): JTAGTransport | undefined {
    return this.dynamicRegistry.getTransport(purpose);
  }

  /**
   * Override: Route to remote node with dynamic P2P transport
   * ENHANCES: Parent's routeToRemoteNode with dynamic transport access
   */
  protected async routeToRemoteNode(
    message: JTAGMessage, 
    remoteInfo: { nodeId: string; targetPath: string }
  ): Promise<RouterResult> {
    const { nodeId, targetPath } = remoteInfo;
    
    console.log(`🌐 ${this.toString()}: Dynamic routing to remote node ${nodeId} → ${targetPath}`);

    // Use dynamic registry instead of hardcoded transport map
    const p2pTransport = this.dynamicRegistry.getTransport('p2p');

    if (!p2pTransport) {
      throw new Error(`No P2P transport available for remote routing to node ${nodeId}`);
    }

    // Create modified message with target path
    const remoteMessage: JTAGMessage = {
      ...message,
      endpoint: targetPath
    };

    try {
      // Use parent's message type checking and correlation logic
      if (message.type === 'request') {
        console.log(`🎯 ${this.toString()}: Sending P2P request to ${nodeId}`);
        
        // Use parent's response correlator (don't duplicate this logic)
        const responsePromise = (this as any).responseCorrelator.createRequest(message.correlationId);
        
        await p2pTransport.send(remoteMessage);
        const response = await responsePromise;
        
        console.log(`✅ ${this.toString()}: P2P response received from ${nodeId}`);
        return { success: true, resolved: true, response };
        
      } else {
        console.log(`📢 ${this.toString()}: Sending P2P event to ${nodeId}`);
        await p2pTransport.send(remoteMessage);
        return { success: true, delivered: true };
      }
    } catch (error) {
      console.error(`❌ ${this.toString()}: P2P routing failed to ${nodeId}:`, error);
      throw error;
    }
  }

  /**
   * Add new transport dynamically at runtime
   */
  async addTransport(
    purpose: string,
    protocol: keyof typeof PROTOCOLS,
    config: Partial<ChannelConfig>
  ): Promise<boolean> {
    try {
      console.log(`🔧 Adding dynamic transport: ${purpose} via ${protocol}`);

      const channelConfig: ChannelConfig = {
        id: {
          environment: config.id?.environment || ENVIRONMENTS.REMOTE,
          protocol: PROTOCOLS[protocol],
          target: config.id?.target,
          instanceId: crypto.randomUUID() as any
        },
        transport: {
          protocol: protocol.toLowerCase().replace('_', '-') as any,
          role: 'client',
          eventSystem: this.eventManager.events,
          sessionId: this.context.uuid,
          handler: this,
          ...config.transport
        },
        handler: this,
        ...config
      };
      
      const result = await this.channelManager.createChannel(channelConfig);
      if (result.success) {
        this.dynamicRegistry.register(purpose, result.channel!);
        
        // Set up message handler
        if (result.channel!.transport.setMessageHandler) {
          result.channel!.transport.setMessageHandler((message: JTAGMessage) => {
            this.postMessage(message).catch(console.error);
          });
        }
        
        console.log(`✅ Added dynamic transport: ${purpose} via ${protocol}`);
        return true;
      }
      return false;
    } catch (error) {
      console.error(`❌ Failed to add transport ${purpose}:`, error);
      return false;
    }
  }

  /**
   * Override: Enhanced transport status with dynamic registry info
   */
  getTransportStatus() {
    const baseStatus = super.getTransportStatus();
    const dynamicStats = this.dynamicRegistry.getStats();
    
    return {
      ...baseStatus,
      dynamicTransports: dynamicStats,
      availablePurposes: this.dynamicRegistry.getPurposes(),
      channelManager: this.channelManager ? this.channelManager.getStatus() : null
    };
  }

  /**
   * Override: Shutdown with dynamic registry cleanup
   */
  async shutdownTransports(): Promise<void> {
    console.log(`🔄 ${this.toString()}: Shutting down dynamic transports...`);
    
    // Shutdown all dynamic channels
    for (const purpose of this.dynamicRegistry.getPurposes()) {
      const channel = this.dynamicRegistry.getChannel(purpose);
      if (channel) {
        try {
          await channel.transport.disconnect();
          console.log(`✅ Disconnected ${purpose} transport`);
        } catch (error) {
          console.warn(`⚠️ Error disconnecting ${purpose}:`, error);
        }
      }
    }
    
    this.dynamicRegistry.clear();
    console.log(`✅ ${this.toString()}: Dynamic transport shutdown complete`);
  }

  /**
   * Get dynamic registry for advanced operations
   */
  get transportRegistry(): DynamicTransportRegistry {
    return this.dynamicRegistry;
  }

  /**
   * Get channel manager for advanced channel operations
   */
  get channels(): IChannelManager {
    return this.channelManager;
  }
}

// ============================================================================
// COMPATIBILITY LAYER - MAKES SUBCLASS DROP-IN REPLACEMENT
// ============================================================================

/**
 * Factory function for easy migration
 */
export async function createDynamicRouter(
  context: JTAGContext,
  config: JTAGRouterConfig = {}
): Promise<JTAGRouterDynamic> {
  const router = new JTAGRouterDynamic(context, config);
  await router.initialize();
  return router;
}

/**
 * Type guard to check if router is dynamic
 */
export function isDynamicRouter(router: JTAGRouter): router is JTAGRouterDynamic {
  return router instanceof JTAGRouterDynamic;
}

// ============================================================================
// MIGRATION GUIDE
// ============================================================================

export const MigrationGuide = {
  /**
   * Step 1: Replace router instantiation
   */
  replace: `
    // BEFORE:
    const router = new JTAGRouter(context, config);
    
    // AFTER:
    const router = new JTAGRouterDynamic(context, config);
    // OR
    const router = await createDynamicRouter(context, config);
  `,

  /**
   * Step 2: Test existing functionality
   */
  test: `
    // All existing functionality should work unchanged:
    await router.initialize();
    const result = await router.postMessage(message);
    
    // Screenshot → file/save promise chain should work:
    const screenshot = await jtagSystem.commands.screenshot();
  `,

  /**
   * Step 3: Enable new capabilities
   */
  enhance: `
    // Add custom transports:
    await router.addTransport('video-stream', 'WEBRTC', {
      transport: { /* WebRTC config */ }
    });
    
    // Check P2P availability:
    if (router.transportRegistry.isAvailable('p2p')) {
      console.log('P2P mesh networking enabled');
    }
    
    // Advanced channel operations:
    const channelStatus = router.channels.getStatus();
  `
};`