/**
 * Connection Broker - Intelligent connection management with global awareness
 * 
 * Provides centralized transport orchestration that eliminates port conflicts,
 * enables intelligent server reuse, and supports location-transparent connectivity
 * across browser, server, and P2P environments.
 * 
 * Architecture:
 * - Server Registry: Dual-key (GUID + name) server tracking
 * - Port Pool: Dynamic allocation with conflict detection
 * - Connection Reuse: Smart sharing based on compatibility
 * - P2P Discovery: Automatic mesh network integration
 * - Strong Typing: Complete TypeScript type safety
 */

import { generateUUID, type UUID } from '../../types/CrossPlatformUUID';
import type { JTAGEnvironment } from '../../types/JTAGTypes';
import type { ITransportFactory } from '../../../transports/shared/ITransportFactory';
import type { TransportConfig, TransportProtocol, JTAGTransport } from '../../../transports/shared/TransportTypes';
import {
  type IConnectionBroker,
  type ConnectionParams,
  type ConnectionResult,
  type ServerRegistryEntry,
  type ServerSelector,
  type ConnectionMode,
  type ConnectionStrategy,
  type ConnectionMetadata,
  type PortPoolConfig,
  type PortAllocationStrategy,
  type ServerStatus,
  type ConnectionBrokerConfig,
  type BrokerStatistics,
  ConnectionBrokerError,
  DEFAULT_BROKER_CONFIG
} from './ConnectionBrokerTypes';

/**
 * Central Connection Broker - orchestrates all JTAG transport connections
 * 
 * This class eliminates the hardcoded connection logic in JTAGClient by providing
 * intelligent, globally-aware connection management. It tracks all running servers,
 * manages port allocation, and enables connection reuse across multiple clients.
 */
export class ConnectionBroker implements IConnectionBroker {
  private readonly config: ConnectionBrokerConfig;
  private readonly serverRegistry = new Map<UUID, ServerRegistryEntry>();
  private readonly portAllocations = new Map<number, UUID>(); // port -> server GUID
  private readonly nameIndex = new Map<string, UUID>(); // name -> server GUID
  private readonly statistics: BrokerStatistics;
  
  private cleanupTimer?: NodeJS.Timeout;
  private persistTimer?: NodeJS.Timeout;

  constructor(
    config: Partial<ConnectionBrokerConfig> = {},
    private readonly transportFactory: ITransportFactory
  ) {
    this.config = this.mergeConfig(config);
    this.statistics = this.initializeStatistics();
    
    this.startMaintenanceTimers();
  }

  /**
   * Main connection establishment method - implements intelligent connection routing
   * 
   * Decision Flow:
   * 1. Check for existing compatible servers (if mode allows reuse)
   * 2. Attempt P2P discovery (if P2P protocols requested)
   * 3. Create new server instance (if mode allows)
   * 4. Apply fallback strategies if needed
   */
  async connect(params: ConnectionParams): Promise<ConnectionResult> {
    const startTime = Date.now();
    let retryAttempts = 0;
    const maxRetries = params.maxRetries ?? 3;
    
    console.log(`🔗 ConnectionBroker: Processing connection request for ${params.targetEnvironment} (mode: ${params.mode})`);

    while (retryAttempts <= maxRetries) {
      try {
        // Phase 1: Try connection reuse (if mode allows)
        if (this.shouldAttemptReuse(params.mode)) {
          const existingServer = await this.findCompatibleServer(params);
          if (existingServer) {
            const result = await this.connectToExistingServer(existingServer, params, startTime, retryAttempts);
            this.updateStatistics(result);
            return result;
          }
        }

        // Phase 2: Try P2P discovery (if P2P protocols requested)
        if (this.hasP2PProtocols(params.protocols) && this.config.p2pDiscovery) {
          try {
            const discoveredServer = await this.discoverP2PServer(params);
            if (discoveredServer) {
              const result = await this.connectToDiscoveredServer(discoveredServer, params, startTime, retryAttempts);
              this.updateStatistics(result);
              return result;
            }
          } catch (error) {
            console.warn(`🔍 ConnectionBroker: P2P discovery failed:`, error);
            // Continue to next phase - P2P is optional
          }
        }

        // Phase 3: Create new server (if mode allows)
        if (this.shouldCreateNewServer(params.mode)) {
          const result = await this.createNewConnection(params, startTime, retryAttempts, params.eventSystem, params.handler);
          this.updateStatistics(result);
          return result;
        }

        // Phase 4: Apply fallback strategies
        if (params.enableFallback && retryAttempts < maxRetries) {
          console.log(`🔄 ConnectionBroker: Applying fallback strategies (attempt ${retryAttempts + 1}/${maxRetries})`);
          const fallbackResult = await this.applyFallbackStrategy(params, startTime, retryAttempts);
          if (fallbackResult) {
            this.updateStatistics(fallbackResult);
            return fallbackResult;
          }
        }

        throw new ConnectionBrokerError(
          `Unable to establish connection after ${retryAttempts} attempts`,
          'CONNECTION_TIMEOUT',
          { params, attempts: retryAttempts }
        );

      } catch (error) {
        retryAttempts++;
        if (retryAttempts > maxRetries) {
          this.statistics.totalConnections++;
          throw error;
        }
        
        const delay = this.calculateRetryDelay(retryAttempts);
        console.log(`⏳ ConnectionBroker: Retrying in ${delay}ms (attempt ${retryAttempts}/${maxRetries})`);
        await this.delay(delay);
      }
    }

    throw new ConnectionBrokerError('Maximum retry attempts exceeded', 'CONNECTION_TIMEOUT');
  }

  /**
   * Register new server in the broker registry
   */
  async registerServer(
    serverData: Omit<ServerRegistryEntry, 'guid' | 'createdAt' | 'lastActivity' | 'connectionCount' | 'status'>
  ): Promise<UUID> {
    const guid = generateUUID();
    const now = new Date();

    // Validate port availability
    if (this.portAllocations.has(serverData.port)) {
      const existingServer = this.portAllocations.get(serverData.port);
      throw new ConnectionBrokerError(
        `Port ${serverData.port} already allocated to server ${existingServer}`,
        'PORT_CONFLICT',
        { port: serverData.port, existingServer }
      );
    }

    const server: ServerRegistryEntry = {
      guid,
      name: serverData.name,
      port: serverData.port,
      protocol: serverData.protocol,
      environment: serverData.environment,
      createdAt: now,
      lastActivity: now,
      connectionCount: 0,
      status: 'starting',
      processId: serverData.processId,
      capabilities: serverData.capabilities,
      metadata: serverData.metadata,
      tags: serverData.tags
    };

    this.serverRegistry.set(guid, server);
    this.portAllocations.set(serverData.port, guid);
    
    if (serverData.name) {
      this.nameIndex.set(serverData.name, guid);
    }

    console.log(`📋 ConnectionBroker: Registered server ${serverData.name} (${guid}) on port ${serverData.port}`);
    return guid;
  }

  /**
   * Unregister server from broker
   */
  async unregisterServer(serverGuid: UUID): Promise<void> {
    const server = this.serverRegistry.get(serverGuid);
    if (!server) {
      console.warn(`⚠️ ConnectionBroker: Attempted to unregister unknown server ${serverGuid}`);
      return;
    }

    // Update server status and cleanup indexes
    server.status = 'stopped';
    this.portAllocations.delete(server.port);
    if (server.name) {
      this.nameIndex.delete(server.name);
    }
    this.serverRegistry.delete(serverGuid);

    console.log(`🗑️ ConnectionBroker: Unregistered server ${server.name} (${serverGuid})`);
  }

  /**
   * Find servers matching selection criteria
   */
  async findServers(selector?: ServerSelector, mode?: ConnectionMode): Promise<readonly ServerRegistryEntry[]> {
    const allServers = Array.from(this.serverRegistry.values());
    
    if (!selector) {
      return allServers.filter(server => server.status === 'ready');
    }

    let matchingServers = allServers;

    // Filter by GUID if specified
    if (selector.guid) {
      matchingServers = matchingServers.filter(server => server.guid === selector.guid);
    }

    // Filter by name if specified
    if (selector.name) {
      matchingServers = matchingServers.filter(server => server.name === selector.name);
    }

    // Filter by port if specified
    if (selector.port) {
      matchingServers = matchingServers.filter(server => server.port === selector.port);
    }

    // Filter by tags if specified
    if (selector.tags && selector.tags.length > 0) {
      matchingServers = matchingServers.filter(server => 
        server.tags && selector.tags!.every(tag => server.tags!.includes(tag))
      );
    }

    // Filter by status - only return available servers
    matchingServers = matchingServers.filter(server => 
      server.status === 'ready' || (mode === 'shared' && server.status === 'busy')
    );

    return matchingServers;
  }

  /**
   * Get current registry state for diagnostics
   */
  async getRegistryState(): Promise<{
    servers: readonly ServerRegistryEntry[];
    portAllocations: ReadonlyMap<number, UUID>;
    statistics: BrokerStatistics;
  }> {
    return {
      servers: Array.from(this.serverRegistry.values()),
      portAllocations: new Map(this.portAllocations),
      statistics: { ...this.statistics }
    };
  }

  /**
   * Cleanup inactive servers and ports
   */
  async cleanup(): Promise<void> {
    const now = Date.now();
    const maxAge = this.config.registry.maxEntryAge;
    let cleanedCount = 0;

    for (const [guid, server] of this.serverRegistry) {
      const age = now - server.lastActivity.getTime();
      
      // Remove stale servers
      if (age > maxAge || server.status === 'stopped' || server.status === 'error') {
        await this.unregisterServer(guid);
        cleanedCount++;
      }
    }

    if (cleanedCount > 0) {
      console.log(`🧹 ConnectionBroker: Cleaned up ${cleanedCount} inactive servers`);
    }
  }

  // Private helper methods

  private shouldAttemptReuse(mode: ConnectionMode): boolean {
    return mode === 'shared' || mode === 'preferred' || mode === 'required';
  }

  private shouldCreateNewServer(mode: ConnectionMode): boolean {
    return mode !== 'required'; // 'required' mode must reuse existing servers
  }

  private hasP2PProtocols(protocols: readonly TransportProtocol[]): boolean {
    return protocols.includes('udp-multicast');
  }

  private async findCompatibleServer(params: ConnectionParams): Promise<ServerRegistryEntry | null> {
    const servers = await this.findServers(params.server, params.mode);
    
    // Find server with matching environment and protocol
    const compatibleServer = servers.find(server => 
      server.environment === params.targetEnvironment &&
      params.protocols.includes(server.protocol) &&
      (params.mode === 'shared' || server.connectionCount === 0)
    );

    return compatibleServer ?? null;
  }

  private async connectToExistingServer(
    server: ServerRegistryEntry, 
    params: ConnectionParams,
    startTime: number,
    retryAttempts: number
  ): Promise<ConnectionResult> {
    console.log(`🔄 ConnectionBroker: Reusing existing server ${server.name} (${server.guid})`);

    const transport = await this.createTransportToServer(server, params, params.eventSystem, params.handler);
    
    // Update server connection count and activity
    server.connectionCount++;
    server.lastActivity = new Date();
    
    const metadata: ConnectionMetadata = {
      establishmentTimeMs: Date.now() - startTime,
      retryAttempts,
      protocolUsed: server.protocol,
      usedFallback: false,
      diagnostics: { serverReused: true, serverGuid: server.guid }
    };

    return {
      transport,
      server,
      strategy: 'reused_existing',
      metadata
    };
  }

  private async createNewConnection(
    params: ConnectionParams,
    startTime: number,
    retryAttempts: number,
    eventSystem: any, // TODO: Replace with proper EventsInterface type
    handler: any // TODO: Replace with proper ITransportHandler type
  ): Promise<ConnectionResult> {
    console.log(`🔗 ConnectionBroker: Creating client connection to existing server`);

    // Connect to existing JTAG system server (assumes system is already running)
    const systemConfig = { getWebSocketPort: () => 9001, getWebSocketUrl: () => 'ws://localhost:9001' };
    const port = systemConfig.getWebSocketPort();
    const protocol = params.protocols[0]; // Use first preferred protocol

    // Register the existing JTAG server in our registry first
    let serverGuid: string;
    const existingServers = await this.findServers({ port });
    
    if (existingServers.length > 0) {
      // Server already registered, reuse it
      serverGuid = existingServers[0].guid;
      existingServers[0].connectionCount++;
    } else {
      // Register the existing JTAG server
      serverGuid = await this.registerServer({
        name: `existing-jtag-server-${port}`,
        port,
        protocol,
        environment: params.targetEnvironment,
        processId: undefined, // External process
        capabilities: params.capabilities,
        metadata: { ...params.metadata, external: true }
      });
      
      const server = this.serverRegistry.get(serverGuid)!;
      server.status = 'ready';
      server.connectionCount = 1;
    }

    const server = this.serverRegistry.get(serverGuid)!;
    
    // Create client transport with properly typed configuration
    const transport = await this.createTransportToServer(server, params, eventSystem, handler);

    const metadata: ConnectionMetadata = {
      establishmentTimeMs: Date.now() - startTime,
      retryAttempts,
      protocolUsed: protocol,
      usedFallback: false,
      diagnostics: { connectedToExisting: true, serverPort: port }
    };

    return {
      transport,
      server,
      strategy: 'created_new',
      metadata
    };
  }

  private async discoverP2PServer(params: ConnectionParams): Promise<ServerRegistryEntry | null> {
    console.log(`🔍 ConnectionBroker: Attempting P2P server discovery`);
    // TODO: Implement P2P discovery using UDP multicast
    // This would integrate with the existing UDP multicast transport
    return null;
  }

  private async connectToDiscoveredServer(
    server: ServerRegistryEntry,
    params: ConnectionParams,
    startTime: number,
    retryAttempts: number
  ): Promise<ConnectionResult> {
    const transport = await this.createTransportToServer(server, params, params.eventSystem, params.handler);

    const metadata: ConnectionMetadata = {
      establishmentTimeMs: Date.now() - startTime,
      retryAttempts,
      protocolUsed: server.protocol,
      usedFallback: false,
      diagnostics: { discoveredViaP2P: true }
    };

    return {
      transport,
      server,
      strategy: 'discovered_p2p',
      metadata
    };
  }

  private async applyFallbackStrategy(
    params: ConnectionParams,
    startTime: number,
    retryAttempts: number
  ): Promise<ConnectionResult | null> {
    // Try alternative protocols
    for (let i = 1; i < params.protocols.length; i++) {
      const fallbackProtocol = params.protocols[i];
      
      try {
        console.log(`🔄 ConnectionBroker: Trying fallback protocol: ${fallbackProtocol}`);
        
        const fallbackParams: ConnectionParams = {
          ...params,
          protocols: [fallbackProtocol] // Use only the fallback protocol
        };
        
        const result = await this.createNewConnection(fallbackParams, startTime, retryAttempts, params.eventSystem, params.handler);
        
        return {
          ...result,
          strategy: 'fallback_protocol',
          metadata: {
            ...result.metadata,
            usedFallback: true,
            protocolUsed: fallbackProtocol
          }
        };
      } catch (error) {
        console.warn(`⚠️ ConnectionBroker: Fallback protocol ${fallbackProtocol} failed:`, error);
        continue;
      }
    }

    return null;
  }

  private async createTransportToServer(
    server: ServerRegistryEntry, 
    params: ConnectionParams,
    eventSystem: any, // TODO: Replace with proper EventsInterface type
    handler: any // TODO: Replace with proper ITransportHandler type
  ): Promise<JTAGTransport> {
    // Create properly typed TransportConfig with all required fields
    const transportConfig: TransportConfig = {
      protocol: server.protocol,
      role: 'client',
      serverPort: server.port,
      serverUrl: `ws://localhost:${server.port}`,
      sessionId: params.sessionId,
      eventSystem: eventSystem, // Required field provided by caller
      handler: handler, // Required field provided by caller
      fallback: true
    };

    return await this.transportFactory.createTransport(params.targetEnvironment, transportConfig);
  }

  private async allocatePort(): Promise<number> {
    const { startPort, endPort, allocationStrategy } = this.config.portPool;
    
    switch (allocationStrategy) {
      case 'sequential':
        return this.allocateSequentialPort(startPort, endPort);
      case 'random':
        return this.allocateRandomPort(startPort, endPort);
      default:
        return this.allocateSequentialPort(startPort, endPort);
    }
  }

  private allocateSequentialPort(start: number, end: number): number {
    for (let port = start; port <= end; port++) {
      if (!this.portAllocations.has(port) && !this.config.portPool.reservedPorts.includes(port)) {
        return port;
      }
    }
    
    throw new ConnectionBrokerError(
      `No available ports in range ${start}-${end}`,
      'NO_AVAILABLE_PORTS',
      { range: [start, end], allocated: Array.from(this.portAllocations.keys()) }
    );
  }

  private allocateRandomPort(start: number, end: number): number {
    const availablePorts: number[] = [];
    
    for (let port = start; port <= end; port++) {
      if (!this.portAllocations.has(port) && !this.config.portPool.reservedPorts.includes(port)) {
        availablePorts.push(port);
      }
    }
    
    if (availablePorts.length === 0) {
      throw new ConnectionBrokerError(
        `No available ports in range ${start}-${end}`,
        'NO_AVAILABLE_PORTS',
        { range: [start, end], allocated: Array.from(this.portAllocations.keys()) }
      );
    }
    
    const randomIndex = Math.floor(Math.random() * availablePorts.length);
    return availablePorts[randomIndex];
  }

  private calculateRetryDelay(attempt: number): number {
    const baseDelay = this.config.timeouts.retryDelay;
    return Math.min(baseDelay * Math.pow(2, attempt - 1), 10000); // Exponential backoff capped at 10s
  }

  private delay(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  private mergeConfig(partial: Partial<ConnectionBrokerConfig>): ConnectionBrokerConfig {
    return {
      portPool: { ...DEFAULT_BROKER_CONFIG.portPool, ...partial.portPool },
      registry: { ...DEFAULT_BROKER_CONFIG.registry, ...partial.registry },
      timeouts: { ...DEFAULT_BROKER_CONFIG.timeouts, ...partial.timeouts },
      p2pDiscovery: partial.p2pDiscovery ? 
        { ...DEFAULT_BROKER_CONFIG.p2pDiscovery, ...partial.p2pDiscovery } :
        DEFAULT_BROKER_CONFIG.p2pDiscovery
    };
  }

  private initializeStatistics(): BrokerStatistics {
    return {
      totalConnections: 0,
      successfulConnections: 0,
      reuseRate: 0,
      avgConnectionTime: 0,
      serversByProtocol: new Map(),
      portUtilization: {
        total: this.config.portPool.endPort - this.config.portPool.startPort + 1,
        allocated: 0,
        available: this.config.portPool.endPort - this.config.portPool.startPort + 1
      }
    };
  }

  private updateStatistics(result: ConnectionResult): void {
    this.statistics.totalConnections++;
    this.statistics.successfulConnections++;
    
    // Update reuse rate
    const reuseCount = result.strategy === 'reused_existing' ? 1 : 0;
    this.statistics.reuseRate = (this.statistics.reuseRate * (this.statistics.successfulConnections - 1) + reuseCount) / 
                                this.statistics.successfulConnections;
    
    // Update average connection time
    const currentAvg = this.statistics.avgConnectionTime;
    const newTime = result.metadata.establishmentTimeMs;
    this.statistics.avgConnectionTime = (currentAvg * (this.statistics.successfulConnections - 1) + newTime) / 
                                       this.statistics.successfulConnections;

    // Update port utilization
    this.statistics.portUtilization = {
      total: this.config.portPool.endPort - this.config.portPool.startPort + 1,
      allocated: this.portAllocations.size,
      available: this.config.portPool.endPort - this.config.portPool.startPort + 1 - this.portAllocations.size
    };
  }

  private startMaintenanceTimers(): void {
    // Registry cleanup timer
    this.cleanupTimer = setInterval(() => {
      this.cleanup().catch(error => 
        console.error('ConnectionBroker cleanup failed:', error)
      );
    }, this.config.registry.cleanupInterval);

    // Registry persistence timer (if persistence is configured)
    if (this.config.registry.filePath) {
      this.persistTimer = setInterval(() => {
        this.persistRegistry().catch(error =>
          console.error('ConnectionBroker persistence failed:', error)
        );
      }, this.config.registry.persistInterval);
    }
  }

  private async persistRegistry(): Promise<void> {
    // TODO: Implement registry persistence to file system
    // This would save the current registry state to disk for recovery
    console.log('📁 ConnectionBroker: Registry persistence not yet implemented');
  }

  /**
   * Cleanup method for graceful shutdown
   */
  async shutdown(): Promise<void> {
    console.log('🛑 ConnectionBroker: Shutting down...');
    
    if (this.cleanupTimer) {
      clearInterval(this.cleanupTimer);
    }
    
    if (this.persistTimer) {
      clearInterval(this.persistTimer);
    }
    
    // Final cleanup of all servers
    await this.cleanup();
    
    console.log('✅ ConnectionBroker: Shutdown complete');
  }
}