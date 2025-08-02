/**
 * Universal Promise-Aware Router
 * 
 * REPLACES: Hardcoded JTAGRouter with cross-cutting concerns
 * ENABLES: Generic P2P command orchestration with promise correlation
 * 
 * DESIGN PRINCIPLES:
 * - Location transparency: Same API for local/remote commands
 * - Promise preservation: Async/await works across network hops
 * - Error propagation: Rejections bubble back to original caller
 * - Protocol agnostic: WebSocket, UDP, HTTP, WebRTC support
 * - Mesh capable: Multi-hop routing with automatic failover
 * 
 * ARCHITECTURE:
 * Browser → Router → Channel → Transport → Remote Router → Command
 *                                                              ↓
 * Promise ← Response ← Channel ← Transport ← Response ← Command Result
 */

import type { JTAGContext, JTAGMessage } from '../types/JTAGTypes';
import type { JTAGResponsePayload } from '../types/ResponseTypes';
import type { UUID } from '../types/CrossPlatformUUID';
import type { 
  IChannelManager,
  TypedEndpoint,
  RoutingContext,
  Environment,
  Protocol,
  Endpoints
} from '../../channels/shared/ChannelTypes';

// ============================================================================
// UNIVERSAL COMMAND INTERFACES
// ============================================================================

/**
 * Universal command request - works across any continuum
 */
export interface UniversalCommandRequest {
  readonly id: UUID;
  readonly command: string;
  readonly target?: TypedEndpoint | string; // 'remote/laptop-node' or endpoint object
  readonly payload: Record<string, any>;
  readonly timeout?: number;
  readonly priority?: CommandPriority;
  readonly requiresResponse: boolean;
  readonly correlationId: UUID; // For promise tracking
  readonly originatingEndpoint: TypedEndpoint;
}

/**
 * Universal command response - correlates back to original promise
 */
export interface UniversalCommandResponse {
  readonly id: UUID;
  readonly correlationId: UUID; // Links back to original request
  readonly success: boolean;
  readonly payload?: any;
  readonly error?: CommandError;
  readonly executedAt: TypedEndpoint;
  readonly hops: RouteHop[]; // Track routing path for debugging
}

/**
 * Command execution error with network context
 */
export interface CommandError {
  readonly code: string;
  readonly message: string;
  readonly cause?: Error;
  readonly occurredAt: TypedEndpoint;
  readonly networkLatency?: number;
}

/**
 * Route hop tracking for mesh debugging
 */
export interface RouteHop {
  readonly endpoint: TypedEndpoint;
  readonly timestamp: number;
  readonly latency?: number;
  readonly protocol: Protocol;
}

/**
 * Command priority for mesh routing decisions
 */
export const COMMAND_PRIORITIES = {
  CRITICAL: 0,  // System commands, health checks
  HIGH: 1,      // User commands, screenshots
  NORMAL: 2,    // Background tasks, file operations  
  LOW: 3        // Sync operations, metrics
} as const;

export type CommandPriority = typeof COMMAND_PRIORITIES[keyof typeof COMMAND_PRIORITIES];

// ============================================================================
// PROMISE CORRELATION SYSTEM
// ============================================================================

/**
 * Promise correlation entry - tracks pending promises across network
 */
interface PromiseCorrelation {
  readonly correlationId: UUID;
  readonly originalRequest: UniversalCommandRequest;
  readonly promise: {
    resolve: (value: any) => void;
    reject: (error: Error) => void;
  };
  readonly startTime: number;
  readonly timeout: NodeJS.Timeout;
}

/**
 * Promise Correlation Manager - the heart of cross-network promise handling
 * 
 * RESPONSIBILITY: Ensure promises resolve/reject correctly regardless of network hops
 */
export class PromiseCorrelationManager {
  private pendingPromises = new Map<UUID, PromiseCorrelation>();
  private defaultTimeout = 30000; // 30 seconds

  /**
   * Create a new promise correlation for network command
   */
  createCorrelation<T = any>(
    request: UniversalCommandRequest
  ): { promise: Promise<T>; correlationId: UUID } {
    const correlationId = request.correlationId;
    
    const promise = new Promise<T>((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.rejectCorrelation(correlationId, new Error(
          `Command timeout after ${request.timeout || this.defaultTimeout}ms: ${request.command}`
        ));
      }, request.timeout || this.defaultTimeout);

      const correlation: PromiseCorrelation = {
        correlationId,
        originalRequest: request,
        promise: { resolve, reject },
        startTime: Date.now(),
        timeout
      };

      this.pendingPromises.set(correlationId, correlation);
    });

    return { promise, correlationId };
  }

  /**
   * Resolve a promise correlation with response
   */
  resolveCorrelation(correlationId: UUID, response: UniversalCommandResponse): boolean {
    const correlation = this.pendingPromises.get(correlationId);
    if (!correlation) {
      console.warn(`⚠️ No pending promise found for correlation ${correlationId}`);
      return false;
    }

    clearTimeout(correlation.timeout);
    this.pendingPromises.delete(correlationId);

    if (response.success) {
      correlation.promise.resolve(response.payload);
    } else {
      const error = new Error(response.error?.message || 'Command failed');
      (error as any).code = response.error?.code;
      (error as any).networkContext = response.error;
      correlation.promise.reject(error);
    }

    return true;
  }

  /**
   * Reject a promise correlation with error
   */
  rejectCorrelation(correlationId: UUID, error: Error): boolean {
    const correlation = this.pendingPromises.get(correlationId);
    if (!correlation) {
      return false;
    }

    clearTimeout(correlation.timeout);
    this.pendingPromises.delete(correlationId);
    correlation.promise.reject(error);
    return true;
  }

  /**
   * Get correlation statistics
   */
  getStats(): { pending: number; totalProcessed: number } {
    return {
      pending: this.pendingPromises.size,
      totalProcessed: -1 // Would track this in real implementation
    };
  }
}

// ============================================================================
// UNIVERSAL ROUTER IMPLEMENTATION
// ============================================================================

/**
 * Universal Router - Generic, P2P-capable command orchestration
 * 
 * REPLACES: Hardcoded router with cross-cutting concerns
 * ENABLES: Location-transparent promise-based command execution
 */
export class UniversalRouter {
  private context: JTAGContext;
  private channelManager: IChannelManager;
  private correlationManager = new PromiseCorrelationManager();
  private routingTable = new Map<string, TypedEndpoint>(); // For mesh routing

  constructor(context: JTAGContext, channelManager: IChannelManager) {
    this.context = context;
    this.channelManager = channelManager;
  }

  /**
   * Execute command with universal routing and promise correlation
   * 
   * CORE MAGIC: This method handles local/remote/mesh routing transparently
   */
  async executeCommand<T = any>(
    command: string,
    payload: Record<string, any> = {},
    options: {
      target?: TypedEndpoint | string;
      timeout?: number;
      priority?: CommandPriority;
    } = {}
  ): Promise<T> {
    const request: UniversalCommandRequest = {
      id: this.generateId(),
      command,
      target: this.resolveTarget(options.target),
      payload,
      timeout: options.timeout,
      priority: options.priority || COMMAND_PRIORITIES.NORMAL,
      requiresResponse: true,
      correlationId: this.generateId(),
      originatingEndpoint: this.getCurrentEndpoint()
    };

    console.log(`🎯 UniversalRouter: Executing ${command} → ${this.formatTarget(request.target)}`);

    // Create promise correlation BEFORE routing
    const { promise } = this.correlationManager.createCorrelation<T>(request);

    try {
      // Route the command through appropriate channel
      await this.routeCommand(request);
      
      // Return the promise - it will resolve when response comes back
      return promise;
    } catch (error) {
      // If routing fails, reject the correlation
      this.correlationManager.rejectCorrelation(request.correlationId, error as Error);
      throw error;
    }
  }

  /**
   * Handle incoming command responses and resolve promises
   */
  handleCommandResponse(response: UniversalCommandResponse): void {
    console.log(`📨 UniversalRouter: Received response for ${response.correlationId}`);
    
    const resolved = this.correlationManager.resolveCorrelation(
      response.correlationId, 
      response
    );

    if (!resolved) {
      console.warn(`⚠️ Received response for unknown correlation: ${response.correlationId}`);
    }
  }

  /**
   * Route command through appropriate channel/transport
   */
  private async routeCommand(request: UniversalCommandRequest): Promise<void> {
    const destination = request.target || this.getLocalEndpoint(request.command);
    
    if (!destination) {
      throw new Error(`No routing destination found for command: ${request.command}`);
    }

    // Convert to JTAG message for transport
    const message: JTAGMessage = {
      id: request.id,
      type: 'command',
      route: Endpoints.toString(destination),
      payload: {
        command: request.command,
        data: request.payload,
        correlationId: request.correlationId,
        originatingEndpoint: request.originatingEndpoint
      },
      timestamp: new Date().toISOString()
    };

    // Route through channel manager
    const routingContext: RoutingContext = {
      source: this.getCurrentEndpoint(),
      destination,
      message,
      requiresResponse: request.requiresResponse,
      timeout: request.timeout,
      priority: request.priority
    };

    const result = await this.channelManager.routeMessage(routingContext);
    
    if (!result.success) {
      throw new Error(`Routing failed: ${result.error?.message}`);
    }

    console.log(`✅ UniversalRouter: Command routed successfully via ${result.resolution?.channel.id.protocol}`);
  }

  /**
   * Resolve target string to typed endpoint
   */
  private resolveTarget(target?: TypedEndpoint | string): TypedEndpoint | undefined {
    if (!target) return undefined;
    
    if (typeof target === 'string') {
      // Parse string like 'remote/laptop-node' or 'local/server'
      return Endpoints.fromString(target);
    }
    
    return target;
  }

  /**
   * Get local endpoint for command execution
   */
  private getLocalEndpoint(command: string): TypedEndpoint {
    // Route to appropriate local environment based on command
    const environment = this.context.environment === 'browser' ? 'browser' : 'server';
    return Endpoints.local[environment as 'browser' | 'server'](command);
  }

  /**
   * Get current endpoint for this router
   */
  private getCurrentEndpoint(): TypedEndpoint {
    const environment = this.context.environment === 'browser' ? 'browser' : 'server';
    return Endpoints.local[environment as 'browser' | 'server']('router');
  }

  /**
   * Format target for logging
   */
  private formatTarget(target?: TypedEndpoint): string {
    return target ? Endpoints.toString(target) : 'local';
  }

  /**
   * Generate unique ID
   */
  private generateId(): UUID {
    return crypto.randomUUID() as UUID;
  }

  /**
   * Get router statistics
   */
  getStats() {
    return {
      correlationManager: this.correlationManager.getStats(),
      channelManager: this.channelManager.getStatus(),
      routingTable: this.routingTable.size
    };
  }
}

// ============================================================================
// COMMAND EXECUTION HELPERS
// ============================================================================

/**
 * Universal command execution interface
 * Provides the clean API that replaces hardcoded router calls
 */
export interface IUniversalCommandExecutor {
  /**
   * Execute any command with universal routing
   */
  execute<T = any>(
    command: string, 
    payload?: Record<string, any>, 
    options?: { target?: string; timeout?: number }
  ): Promise<T>;

  /**
   * Convenience methods for common command patterns
   */
  screenshot(options?: { target?: string; querySelector?: string }): Promise<string>;
  fileSave(options: { target?: string; filename: string; content: string }): Promise<boolean>;
  chat(options: { target?: string; message: string }): Promise<string>;
}

/**
 * Universal Command Executor Implementation
 * Clean interface for universal command execution
 */
export class UniversalCommandExecutor implements IUniversalCommandExecutor {
  constructor(private router: UniversalRouter) {}

  async execute<T = any>(
    command: string, 
    payload: Record<string, any> = {}, 
    options: { target?: string; timeout?: number } = {}
  ): Promise<T> {
    return this.router.executeCommand<T>(command, payload, options);
  }

  async screenshot(options: { target?: string; querySelector?: string } = {}): Promise<string> {
    return this.execute('screenshot', { querySelector: options.querySelector }, {
      target: options.target
    });
  }

  async fileSave(options: { target?: string; filename: string; content: string }): Promise<boolean> {
    return this.execute('file/save', { 
      filename: options.filename, 
      content: options.content 
    }, {
      target: options.target
    });
  }

  async chat(options: { target?: string; message: string }): Promise<string> {
    return this.execute('chat/send-message', { message: options.message }, {
      target: options.target
    });
  }
}