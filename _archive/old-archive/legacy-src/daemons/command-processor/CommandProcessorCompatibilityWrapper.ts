/**
 * CommandProcessorCompatibilityWrapper - Zero-downtime migration wrapper
 * 
 * Battle-tested compatibility wrapper pattern for surgical architecture migration.
 * Uses the same proven approach as SessionManagerCompatibilityWrapper to enable
 * gradual rollout of the new symmetric daemon architecture.
 * 
 * Architecture Decision:
 * - Default: Use legacy CommandProcessorDaemon (100% compatibility)
 * - Flag enabled: Route to new focused daemons (CommandRouter, CommandExecutor, etc.)
 * - Environment flag: CONTINUUM_ENABLE_COMMAND_MIGRATION=true
 * 
 * Migration Strategy:
 * 1. Install compatibility wrapper (this file) - no behavior change
 * 2. Test new architecture with environment flag
 * 3. Gradually enable for specific commands/routes
 * 4. Full migration after validation
 */

import { BaseDaemon } from '../base/BaseDaemon';
import { DaemonMessage, DaemonResponse } from '../base/DaemonProtocol';
import { DaemonType } from '../base/DaemonTypes';
import { CommandProcessorDaemon } from './CommandProcessorDaemon';

// New modular architecture daemons
import { CommandRouter } from './server/CommandRouter';
import { CommandExecutor } from './server/CommandExecutor';
import { HttpApiHandler } from './server/HttpApiHandler';
import { WebSocketHandler } from './server/WebSocketHandler';

export class CommandProcessorCompatibilityWrapper extends BaseDaemon {
  public readonly name = 'command-processor';
  public readonly version = '1.0.0-wrapper';
  public readonly id: string;
  public readonly daemonType = DaemonType.COMMAND_PROCESSOR;
  public readonly config = {
    name: this.name,
    version: this.version,
    port: 9001,
    autoStart: true,
    dependencies: [],
    healthCheck: { interval: 30000, timeout: 5000, retries: 3 },
    resources: { maxMemory: 512, maxCpu: 80 }
  };

  // Legacy system
  private legacyProcessor: CommandProcessorDaemon;
  
  // New modular architecture (not instantiated until enabled)
  private newArchitecture: {
    router?: CommandRouter;
    executor?: CommandExecutor;
    httpHandler?: HttpApiHandler;
    wsHandler?: WebSocketHandler;
  } = {};

  // Migration control - enable new architecture to fix broken logs
  private readonly migrationEnabled: boolean;
  private readonly enabledRoutes: Set<string> = new Set();

  constructor() {
    super();
    this.id = `${this.name}-wrapper-${Date.now()}`;
    
    // Disable new architecture temporarily to confirm legacy system works
    this.migrationEnabled = false; // Disable for debugging
    
    this.log(`🔄 CommandProcessorCompatibilityWrapper initialized`);
    this.log(`📋 Migration enabled: ${this.migrationEnabled}`);
    
    // Always instantiate legacy system for fallback
    try {
      this.legacyProcessor = new CommandProcessorDaemon();
      this.log(`✅ WRAPPER: Legacy CommandProcessorDaemon instantiated successfully`);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ WRAPPER: Failed to instantiate legacy CommandProcessorDaemon: ${errorMessage}`);
      throw error;
    }
    
    if (this.migrationEnabled) {
      this.initializeNewArchitecture();
    }
  }

  protected async onStart(): Promise<void> {
    this.log(`🚀 Starting ${this.name} compatibility wrapper`);
    
    // Always start legacy system first
    try {
      await this.legacyProcessor.start();
      this.log(`✅ Legacy CommandProcessorDaemon started successfully`);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Failed to start legacy CommandProcessorDaemon: ${errorMessage}`);
      throw error;
    }
    
    if (this.migrationEnabled && this.hasNewArchitecture()) {
      await this.startNewArchitecture();
      this.log(`✅ New modular architecture started`);
    }
    
    this.log(`🎯 CommandProcessorCompatibilityWrapper ready (migration: ${this.migrationEnabled})`);
  }

  protected async onStop(): Promise<void> {
    this.log(`🛑 Stopping ${this.name} compatibility wrapper`);
    
    // Stop new architecture first
    if (this.hasNewArchitecture()) {
      await this.stopNewArchitecture();
    }
    
    // Stop legacy system
    await this.legacyProcessor.stop();
    
    this.log(`✅ CommandProcessorCompatibilityWrapper stopped`);
  }

  getMessageTypes(): string[] {
    // Union of all message types from both systems
    const legacyTypes = this.legacyProcessor.getMessageTypes();
    const newTypes = this.getNewArchitectureMessageTypes();
    // Include critical handle_api type for command routing
    return [...new Set([...legacyTypes, ...newTypes, 'handle_api'])];
  }

  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    this.log(`🎯 WRAPPER: Handling message type: ${message.type} (migration enabled: ${this.migrationEnabled})`);
    
    try {
      // Use new architecture if migration is enabled and available
      if (this.migrationEnabled && this.hasNewArchitecture()) {
        this.log(`🚀 WRAPPER: Routing to new symmetric daemon architecture`);
        return await this.routeToNewArchitecture(message);
      } else {
        this.log(`📤 WRAPPER: Falling back to legacy system`);
        return await this.routeToLegacySystem(message);
      }
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ WRAPPER: Error handling message: ${errorMessage}`);
      
      // Try fallback to legacy system on new architecture failure
      if (this.migrationEnabled) {
        this.log(`🔄 WRAPPER: New architecture failed, trying legacy fallback`);
        try {
          return await this.routeToLegacySystem(message);
        } catch (fallbackError) {
          const fallbackErrorMessage = fallbackError instanceof Error ? fallbackError.message : String(fallbackError);
          this.log(`❌ WRAPPER: Legacy fallback also failed: ${fallbackErrorMessage}`);
        }
      }
      
      return {
        success: false,
        error: `CommandProcessor wrapper failed: ${errorMessage}`
      };
    }
  }

  /**
   * Initialize new modular architecture daemons
   */
  private initializeNewArchitecture(): void {
    this.log(`🏗️ Initializing new modular architecture...`);
    
    try {
      this.newArchitecture.router = new CommandRouter();
      this.newArchitecture.executor = new CommandExecutor();
      this.newArchitecture.httpHandler = new HttpApiHandler();
      this.newArchitecture.wsHandler = new WebSocketHandler();
      
      this.log(`✅ New architecture daemons created`);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Failed to initialize new architecture: ${errorMessage}`);
      // Don't throw - fall back to legacy only
    }
  }

  /**
   * Start all new architecture daemons
   */
  private async startNewArchitecture(): Promise<void> {
    const { router, executor, httpHandler, wsHandler } = this.newArchitecture;
    
    if (router) await router.start();
    if (executor) await executor.start();
    if (httpHandler) await httpHandler.start();
    if (wsHandler) await wsHandler.start();
  }

  /**
   * Stop all new architecture daemons
   */
  private async stopNewArchitecture(): Promise<void> {
    const { router, executor, httpHandler, wsHandler } = this.newArchitecture;
    
    if (wsHandler) await wsHandler.stop();
    if (httpHandler) await httpHandler.stop();
    if (executor) await executor.stop();
    if (router) await router.stop();
  }

  /**
   * Check if new architecture is available
   */
  private hasNewArchitecture(): boolean {
    return !!(this.newArchitecture.router && 
              this.newArchitecture.executor && 
              this.newArchitecture.httpHandler && 
              this.newArchitecture.wsHandler);
  }

  // TODO: Implement routing logic when ready to enable new architecture
  // For now, keeping wrapper minimal to restore browser console forwarding

  /**
   * Route message to new symmetric daemon architecture
   */
  private async routeToNewArchitecture(message: DaemonMessage): Promise<DaemonResponse> {
    this.log(`🚀 WRAPPER: Routing to new architecture: ${message.type}`);
    
    const { executor, httpHandler, wsHandler } = this.newArchitecture;
    
    // Handle legacy message types - route back to legacy for compatibility
    if (message.type === 'handle_api') {
      this.log(`🔄 WRAPPER: Legacy handle_api message - routing to legacy system for now`);
      // The new architecture isn't ready for handle_api messages yet
      // Fall back to legacy system which knows how to handle these
      return await this.routeToLegacySystem(message);
    }
    
    // Route new architecture message types to appropriate daemons
    if (message.type === 'http_request' && httpHandler) {
      this.log(`🌐 WRAPPER: Routing HTTP request to HttpApiHandler`);
      return await (httpHandler as any).handleMessage(message);
    }
    
    if (message.type === 'websocket_message' && wsHandler) {
      this.log(`🔌 WRAPPER: Routing WebSocket message to WebSocketHandler`);
      return await (wsHandler as any).handleMessage(message);
    }
    
    if (message.type.startsWith('command.') && executor) {
      this.log(`⚡ WRAPPER: Routing command.* to CommandExecutor`);
      return await (executor as any).handleMessage(message);
    }
    
    // If no new architecture handler found, fall back to legacy
    this.log(`🔄 WRAPPER: No new architecture handler for ${message.type} - falling back to legacy`);
    return await this.routeToLegacySystem(message);
  }

  /**
   * Route message to legacy CommandProcessorDaemon
   */
  private async routeToLegacySystem(message: DaemonMessage): Promise<DaemonResponse> {
    this.log(`📤 WRAPPER: Routing to legacy system: ${message.type}`);
    
    try {
      // Call the legacy processor's protected handleMessage method
      // Since we're the compatibility wrapper, we can access this safely
      const response = await (this.legacyProcessor as any).handleMessage(message);
      this.log(`✅ WRAPPER: Legacy system responded successfully for ${message.type}`);
      return response;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ WRAPPER: Legacy system failed for ${message.type}: ${errorMessage}`);
      
      // Return a proper error response rather than throwing
      return {
        success: false,
        error: `Legacy system error: ${errorMessage}`,
        data: null
      };
    }
  }


  /**
   * Get message types supported by new architecture
   */
  private getNewArchitectureMessageTypes(): string[] {
    if (!this.hasNewArchitecture()) {
      return [];
    }

    const { router, executor, httpHandler, wsHandler } = this.newArchitecture;
    const types: string[] = [];

    if (router) types.push(...router.getMessageTypes());
    if (executor) types.push(...executor.getMessageTypes());
    if (httpHandler) types.push(...httpHandler.getMessageTypes());
    if (wsHandler) types.push(...wsHandler.getMessageTypes());

    return [...new Set(types)];
  }

  /**
   * Register with WebSocketDaemon to handle command routes - CRITICAL for routing
   */
  public registerWithWebSocketDaemon(wsDaemon: { registerRouteHandler: (pattern: string, daemonName: string, messageType: string) => void }): void {
    // Register handler for API command endpoints - this is why commands were going to renderer!
    wsDaemon.registerRouteHandler('/api/commands/*', this.name, 'handle_api');
    this.log(`🔗 CommandProcessorCompatibilityWrapper registered route: /api/commands/* → ${this.name}::handle_api`);
    
    // Also delegate to legacy system for comprehensive coverage
    if (this.legacyProcessor && 'registerWithWebSocketDaemon' in this.legacyProcessor) {
      this.log(`🔗 Also registering legacy CommandProcessorDaemon routes`);
      (this.legacyProcessor as any).registerWithWebSocketDaemon(wsDaemon);
    }
  }

  /**
   * Get migration status for monitoring
   */
  public getMigrationStatus(): {
    enabled: boolean;
    architectureReady: boolean;
    enabledRoutes: string[];
    routeCount: { legacy: number; new: number };
  } {
    return {
      enabled: this.migrationEnabled,
      architectureReady: this.hasNewArchitecture(),
      enabledRoutes: Array.from(this.enabledRoutes),
      routeCount: {
        legacy: 0, // TODO: Add counters
        new: 0
      }
    };
  }
}