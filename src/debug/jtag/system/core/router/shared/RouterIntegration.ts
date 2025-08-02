/**
 * Router Integration Layer
 * 
 * MIGRATION BRIDGE: Replaces hardcoded JTAGRouter with UniversalRouter
 * BACKWARD COMPATIBILITY: Existing code continues to work
 * FORWARD CAPABILITY: Enables P2P mesh routing
 * 
 * ARCHITECTURE EVOLUTION:
 * 
 * BEFORE (Hardcoded):
 * Browser → WebSocket → Server (hardcoded transport map)
 * 
 * AFTER (Universal):
 * Browser → UniversalRouter → ChannelManager → DynamicTransport → AnyDestination
 * 
 * PROMISE FLOW:
 * 1. Command called: await jtag.commands.screenshot()
 * 2. UniversalRouter creates promise correlation
 * 3. Routes through appropriate channel/transport
 * 4. Response comes back through same channel
 * 5. Promise correlation resolves original promise
 * 6. Caller gets result seamlessly
 */

import type { JTAGContext } from '../types/JTAGTypes';
import type { ITransportFactory } from '../../transports/shared/ITransportFactory';
import type { 
  IChannelManager,
  ChannelId,
  ChannelConfig,
  ActiveChannel,
  PROTOCOLS,
  ENVIRONMENTS
} from '../../channels/shared/ChannelTypes';
import { ChannelManager } from '../../channels/shared/ChannelManager';
import { 
  UniversalRouter, 
  UniversalCommandExecutor,
  type IUniversalCommandExecutor 
} from './UniversalRouter';

// ============================================================================
// INTEGRATION FACADE - REPLACES EXISTING ROUTER
// ============================================================================

/**
 * Universal JTAG Router - Drop-in replacement for hardcoded router
 * 
 * MAINTAINS: Existing API compatibility
 * ADDS: P2P routing, mesh capabilities, generic transports
 */
export class UniversalJTAGRouter {
  private context: JTAGContext;
  private channelManager: IChannelManager;
  private universalRouter: UniversalRouter;
  private commandExecutor: IUniversalCommandExecutor;
  private isInitialized = false;

  constructor(context: JTAGContext, transportFactory: ITransportFactory) {
    this.context = context;
    this.channelManager = new ChannelManager(context, transportFactory);
    this.universalRouter = new UniversalRouter(context, this.channelManager);
    this.commandExecutor = new UniversalCommandExecutor(this.universalRouter);
  }

  /**
   * Initialize router with standard channels
   * REPLACES: Hardcoded transport initialization
   */
  async initialize(): Promise<void> {
    if (this.isInitialized) return;

    console.log(`🔧 UniversalJTAGRouter: Initializing for ${this.context.environment}...`);

    // Create standard WebSocket channel (backward compatibility)
    await this.createWebSocketChannel();

    // TODO: Create additional channels based on configuration
    // - UDP multicast for P2P discovery
    // - HTTP fallback for firewall traversal
    // - WebRTC for direct peer connections

    this.isInitialized = true;
    console.log(`✅ UniversalJTAGRouter: Initialized successfully`);
  }

  /**
   * Execute command with universal routing
   * SAME API: Existing code works unchanged
   * NEW CAPABILITY: Can target any continuum
   */
  async executeCommand<T = any>(
    command: string,
    payload: Record<string, any> = {},
    options: {
      target?: string; // NEW: 'remote/laptop-node', 'local/server', etc.
      timeout?: number;
    } = {}
  ): Promise<T> {
    if (!this.isInitialized) {
      await this.initialize();
    }

    return this.commandExecutor.execute<T>(command, payload, options);
  }

  /**
   * Convenience command methods - SAME API as before
   */
  async screenshot(options: { 
    querySelector?: string; 
    target?: string; // NEW: Optional target specification
  } = {}): Promise<string> {
    return this.commandExecutor.screenshot(options);
  }

  async fileSave(
    filename: string, 
    content: string, 
    target?: string // NEW: Optional target specification
  ): Promise<boolean> {
    return this.commandExecutor.fileSave({ filename, content, target });
  }

  async sendMessage(
    message: string, 
    target?: string // NEW: Optional target specification
  ): Promise<string> {
    return this.commandExecutor.chat({ message, target });
  }

  /**
   * Get router status and statistics
   */
  getStatus() {
    return {
      initialized: this.isInitialized,
      environment: this.context.environment,
      stats: this.universalRouter.getStats()
    };
  }

  // ============================================================================
  // CHANNEL MANAGEMENT - REPLACES HARDCODED TRANSPORTS
  // ============================================================================

  /**
   * Create WebSocket channel for browser/server communication
   * REPLACES: Hardcoded WebSocket transport creation
   */
  private async createWebSocketChannel(): Promise<ActiveChannel> {
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
        eventSystem: this.context.eventSystem,
        sessionId: this.context.sessionId,
        serverUrl: this.context.environment === 'browser' ? 'ws://localhost:9001' : undefined,
        serverPort: this.context.environment === 'server' ? 9001 : undefined,
        handler: {
          transportId: crypto.randomUUID() as any,
          handleTransportMessage: async (message) => {
            // Route incoming messages to universal router
            if (message.payload?.correlationId) {
              this.universalRouter.handleCommandResponse({
                id: message.id as any,
                correlationId: message.payload.correlationId,
                success: message.payload.success !== false,
                payload: message.payload.data,
                error: message.payload.error,
                executedAt: message.payload.executedAt || { environment: 'unknown', path: 'unknown' },
                hops: message.payload.hops || []
              });
            }
            return { success: true, data: null };
          }
        }
      },
      handler: {
        transportId: crypto.randomUUID() as any,
        handleTransportMessage: async (message) => ({ success: true, data: null })
      }
    };

    const result = await this.channelManager.createChannel(channelConfig);
    
    if (!result.success) {
      throw new Error(`Failed to create WebSocket channel: ${result.error?.message}`);
    }

    console.log(`✅ Created WebSocket channel: ${this.context.environment} → ${channelId.environment}`);
    return result.channel!;
  }

  /**
   * Add P2P mesh channel
   * NEW CAPABILITY: Enables cross-continuum communication
   */
  async addMeshChannel(nodeId: string, capabilities: string[] = []): Promise<void> {
    const channelId: ChannelId = {
      environment: ENVIRONMENTS.REMOTE,
      protocol: PROTOCOLS.UDP_MULTICAST,
      target: nodeId,
      instanceId: crypto.randomUUID() as any
    };

    const channelConfig: ChannelConfig = {
      id: channelId,
      transport: {
        protocol: 'udp-multicast',
        role: 'peer',
        eventSystem: this.context.eventSystem,
        sessionId: this.context.sessionId,
        p2p: {
          nodeId,
          nodeType: this.context.environment === 'browser' ? 'browser' : 'server',
          capabilities,
          multicastAddress: '239.255.0.1',
          multicastPort: 9003
        },
        handler: {
          transportId: crypto.randomUUID() as any,
          handleTransportMessage: async (message) => ({ success: true, data: null })
        }
      },
      handler: {
        transportId: crypto.randomUUID() as any,
        handleTransportMessage: async (message) => ({ success: true, data: null })
      }
    };

    const result = await this.channelManager.createChannel(channelConfig);
    
    if (!result.success) {
      throw new Error(`Failed to create mesh channel to ${nodeId}: ${result.error?.message}`);
    }

    console.log(`✅ Added mesh channel to node: ${nodeId}`);
  }
}

// ============================================================================
// MIGRATION EXAMPLES
// ============================================================================

/**
 * Migration Examples - Before vs After
 */
export const MigrationExamples = {
  /**
   * BEFORE: Hardcoded router with limited flexibility
   */
  oldWay: `
    // Old hardcoded approach - limited to browser/server
    const result = await jtagRouter.executeCommand('screenshot', { querySelector: 'body' });
    
    // Hardcoded transport mapping, no P2P support
    // Cross-cutting concerns in transport layer
    // No error propagation across network hops
  `,

  /**
   * AFTER: Universal router with full mesh capabilities  
   */
  newWay: `
    // Same API - existing code works unchanged
    const result = await universalRouter.executeCommand('screenshot', { querySelector: 'body' });
    
    // NEW: Target any continuum in the mesh
    const remoteResult = await universalRouter.executeCommand('screenshot', 
      { querySelector: 'body' }, 
      { target: 'remote/laptop-node' }
    );
    
    // NEW: Chain commands across multiple continuums
    const aiResponse = await universalRouter.executeCommand('chat/send-message',
      { message: 'Process this image', image: remoteResult },
      { target: 'remote/ai-server' }
    );
    
    // Promise semantics work seamlessly across network hops
    // Error handling propagates back to original caller
    // Generic transport system supports any protocol
  `
};

// ============================================================================
// FACTORY FOR EASY INTEGRATION
// ============================================================================

/**
 * Create Universal JTAG Router with sensible defaults
 * EASY MIGRATION: One-line replacement for existing router
 */
export async function createUniversalRouter(
  context: JTAGContext, 
  transportFactory: ITransportFactory,
  options: {
    enableMesh?: boolean;
    meshNodeId?: string;
    meshCapabilities?: string[];
  } = {}
): Promise<UniversalJTAGRouter> {
  const router = new UniversalJTAGRouter(context, transportFactory);
  await router.initialize();

  // Optionally enable mesh networking
  if (options.enableMesh && options.meshNodeId) {
    await router.addMeshChannel(
      options.meshNodeId, 
      options.meshCapabilities || []
    );
  }

  return router;
}