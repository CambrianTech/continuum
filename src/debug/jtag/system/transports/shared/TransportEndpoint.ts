// ISSUES: 0 open, last updated 2025-07-30 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Transport Endpoint Interface - Standardized Transport Management
 * 
 * Defines the contract for transport initialization and management across
 * JTAGRouter and JTAGClient. Eliminates duplication and ensures consistent
 * transport lifecycle management throughout the system.
 * 
 * CORE ARCHITECTURE:
 * - Standardized transport initialization patterns
 * - Unified message handler registration
 * - Consistent transport lifecycle management
 * - Environment-agnostic transport configuration
 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: Transport initialization and configuration
 * - Integration tests: Message handler registration and routing
 * - Performance tests: Transport setup and teardown efficiency
 * - Compatibility tests: Cross-environment transport behavior
 * 
 * ARCHITECTURAL INSIGHTS:
 * - Interface enables both JTAGRouter and JTAGClient to reuse transport logic
 * - Separates transport management from business logic
 * - Enables consistent error handling across transport implementations
 * - Supports multiple transport types with unified interface
 */

import type { JTAGMessage, JTAGContext } from '@shared/JTAGTypes';
import type { EventsInterface } from '@systemEvents';
import type { JTAGTransport, TransportConfig } from './TransportTypes';

/**
 * Transport endpoint management interface
 * 
 * Implemented by classes that need to manage transport connections
 * (JTAGRouter, JTAGClient, etc.)
 */
export interface TransportEndpoint {
  /**
   * Initialize transport with configuration
   */
  initializeTransport(config?: TransportConfig): Promise<void>;

  /**
   * Set up message handlers for incoming messages
   */
  setupMessageHandlers(): Promise<void>;

  /**
   * Shutdown all transports and cleanup
   */
  shutdownTransports(): Promise<void>;

  /**
   * Get transport status information
   */
  getTransportStatus(): TransportEndpointStatus;
}

/**
 * Transport endpoint status information
 */
export interface TransportEndpointStatus {
  initialized: boolean;
  transportCount: number;
  transports: Array<{
    name: string;
    connected: boolean;
    type: string;
  }>;
}

/**
 * Configuration for transport endpoint initialization
 */
export interface TransportEndpointConfig {
  context: JTAGContext;
  eventSystem: EventsInterface;
  sessionId?: string;
  transportConfig?: TransportConfig;
  messageHandler?: (message: JTAGMessage) => Promise<void>;
}

/**
 * Abstract base class implementing common transport endpoint patterns
 * 
 * Provides reusable transport management logic that can be extended
 * by JTAGRouter, JTAGClient, and other transport-aware components.
 */
export abstract class TransportEndpointBase implements TransportEndpoint {
  protected readonly context: JTAGContext;
  protected readonly eventSystem: EventsInterface;
  protected readonly transports = new Map<string, JTAGTransport>();
  protected initialized = false;
  protected messageHandler?: (message: JTAGMessage) => Promise<void>;

  constructor(config: TransportEndpointConfig) {
    this.context = config.context;
    this.eventSystem = config.eventSystem;
    this.messageHandler = config.messageHandler;
  }

  /**
   * Template method for transport initialization
   * Subclasses can override specific steps while reusing common logic
   */
  async initializeTransport(config: TransportConfig = {}): Promise<void> {
    if (this.initialized) {
      console.log(`🔄 TransportEndpoint[${this.context.environment}]: Already initialized`);
      return;
    }

    console.log(`🔗 TransportEndpoint[${this.context.environment}]: Initializing transports...`);

    try {
      // Step 1: Create transports (implemented by subclasses)
      await this.createTransports(config);

      // Step 2: Set up message handlers
      await this.setupMessageHandlers();

      // Step 3: Mark as initialized
      this.initialized = true;

      console.log(`✅ TransportEndpoint[${this.context.environment}]: Initialization complete`);
    } catch (error) {
      console.error(`❌ TransportEndpoint[${this.context.environment}]: Initialization failed:`, error);
      throw error;
    }
  }

  /**
   * Set up message handlers for all transports
   */
  async setupMessageHandlers(): Promise<void> {
    if (!this.messageHandler) {
      console.warn(`⚠️ TransportEndpoint[${this.context.environment}]: No message handler provided`);
      return;
    }

    for (const [type, transport] of this.transports) {
      if (transport.setMessageHandler) {
        transport.setMessageHandler(this.messageHandler);
        console.log(`📨 TransportEndpoint[${this.context.environment}]: Message handler set for ${type}`);
      }
    }
  }

  /**
   * Shutdown all transports
   */
  async shutdownTransports(): Promise<void> {
    console.log(`🔄 TransportEndpoint[${this.context.environment}]: Shutting down transports...`);

    for (const [type, transport] of this.transports) {
      try {
        await transport.disconnect();
        console.log(`✅ TransportEndpoint[${this.context.environment}]: ${type} transport disconnected`);
      } catch (error) {
        console.error(`❌ TransportEndpoint[${this.context.environment}]: Failed to disconnect ${type}:`, error);
      }
    }

    this.transports.clear();
    this.initialized = false;

    console.log(`✅ TransportEndpoint[${this.context.environment}]: Shutdown complete`);
  }

  /**
   * Get transport status
   */
  getTransportStatus(): TransportEndpointStatus {
    return {
      initialized: this.initialized,
      transportCount: this.transports.size,
      transports: Array.from(this.transports.entries()).map(([type, transport]) => ({
        name: transport.name,
        connected: transport.isConnected(),
        type
      }))
    };
  }

  /**
   * Get transport by type (strongly typed)
   */
  protected getTransport(type: string): JTAGTransport | undefined {
    return this.transports.get(type);
  }

  /**
   * Add transport with type checking
   */
  protected addTransport(type: string, transport: JTAGTransport): void {
    this.transports.set(type, transport);
    console.log(`📋 TransportEndpoint[${this.context.environment}]: Added ${type} transport: ${transport.name}`);
  }

  /**
   * Abstract method for creating transports
   * Subclasses implement their specific transport creation logic
   */
  protected abstract createTransports(config: TransportConfig): Promise<void>;
}