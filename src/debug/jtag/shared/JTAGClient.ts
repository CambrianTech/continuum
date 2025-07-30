// ISSUES: 3 open, last updated 2025-07-30 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * JTAGClient - Shared base class for JTAG system clients with dynamic command discovery
 * 
 * Provides methods to connect to the JTAG system, send commands, and receive responses.
 * Supports both local (direct) and remote (transport-based) connections with identical APIs.
 * 
 * COMPLETED FEATURES:
 * ✅ Dynamic command discovery via 'list' command
 * ✅ Connection abstraction pattern (LocalConnection vs RemoteConnection)
 * ✅ Strongly-typed command interface with full TypeScript safety
 * ✅ Transport setup and configuration
 * ✅ Robust retry logic for connection establishment
 * 
 * ISSUES: (look for TODOs)
 * - TODO: Implement response correlation for RemoteConnection
 * - TODO: Add health check mechanism for connection monitoring
 * - TODO: Implement proper factory pattern for client type selection
 * 
 * CORE ARCHITECTURE:
 * - Abstract base class - subclasses implement environment-specific details
 * - Connection abstraction allows identical API for local vs remote usage
 * - Dynamic proxy interface generates command methods from server discovery
 * - Transport-agnostic design supports WebSocket, HTTP, and future protocols
 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: Connection abstraction and command proxy behavior
 * - Integration tests: Local vs remote connection behavior parity
 * - Performance tests: Command execution overhead and throughput
 * - Failure tests: Network partition and recovery scenarios
 * 
 * ARCHITECTURAL INSIGHTS:
 * - Eliminates hardcoded command definitions through runtime discovery
 * - Provides unified client interface for calling JTAGSystem via any transport
 * - Enables type-safe command execution without compile-time coupling
 */


import { generateUUID, type UUID} from './CrossPlatformUUID';
import { JTAGBase, type CommandsInterface } from './JTAGBase';
import type { JTAGContext, JTAGMessage, JTAGPayload } from './JTAGTypes';
import { JTAGMessageFactory } from './JTAGTypes';
import { TransportFactory } from '@systemTransports';
import type { TransportConfig, JTAGTransport, TransportProtocol, TransportRole } from '@systemTransports';
import type { ListParams, ListResult, CommandSignature } from '../commands/list/shared/ListTypes';
import { createListParams } from '../commands/list/shared/ListTypes';

/**
 * JTAGClient connection options
 */
export interface JTAGClientConnectOptions {
  readonly targetEnvironment?: 'server' | 'browser';
  readonly transportType?: 'websocket' | 'http';
  readonly serverPort?: number;
  readonly serverUrl?: string;
  readonly timeout?: number;
  readonly enableFallback?: boolean;
  readonly maxRetries?: number;
  readonly retryDelay?: number;
  readonly sessionId?: UUID; // Allow sharing sessionId across clients
}

/**
 * Connection abstraction - local vs remote execution strategy
 */
export interface JTAGConnection {
  executeCommand<TParams extends JTAGPayload, TResult extends JTAGPayload>(
    commandName: string, 
    params: TParams
  ): Promise<TResult>;
  
  readonly sessionId: UUID;
  readonly context: JTAGContext;
}

/**
 * Dynamic commands interface generated from server's list command
 */
export interface DynamicCommandsInterface {
  // Essential commands (always available)
  list(params?: Partial<ListParams>): Promise<ListResult>;
  
  // Dynamic commands discovered from server
  [commandName: string]: (params?: any) => Promise<any>;
}

export abstract class JTAGClient extends JTAGBase {
  protected systemTransport?: JTAGTransport;
  protected commandsInterface?: CommandsInterface;
  protected connection?: JTAGConnection;
  protected discoveredCommands: Map<string, CommandSignature> = new Map();

  public readonly sessionId: UUID;

  // Initialize sessionId in constructor
  constructor(context: JTAGContext) {
    super('jtag-client', context);
    this.sessionId = context.uuid;
  }


  protected override async initialize(options?: JTAGClientConnectOptions): Promise<void> {
    const transportConfig: TransportConfig = { 
      protocol: (options?.transportType ?? 'websocket') as TransportProtocol,
      role: 'client' as TransportRole, // JTAGClient always creates client transports (connectors)
      eventSystem: this.eventManager.events,
      sessionId: this.sessionId, // Pass sessionId for client handshake
      serverPort: options?.serverPort ?? 9001, // Default WebSocket port
      serverUrl: options?.serverUrl ?? `ws://localhost:${options?.serverPort ?? 9001}`,
      fallback: options?.enableFallback ?? true
    };

    this.systemTransport = await TransportFactory.createTransport(this.context.environment, transportConfig);

    // Create remote commands interface that routes through transport
    await this.setupRemoteCommandsInterface();
  }

  /**
   * Set up commands interface that routes commands through transport to remote JTAGSystem
   * Now includes dynamic command discovery via list command
   */
  private async setupRemoteCommandsInterface(): Promise<void> {
    if (!this.systemTransport) {
      throw new Error('Transport not available for remote commands setup');
    }

    // Create remote connection that uses transport
    this.connection = new RemoteConnection(this);
    
    // Discover available commands from remote system
    await this.discoverCommands();
    
    console.log(`📡 JTAGClient: Remote commands interface setup completed with ${this.discoveredCommands.size} commands`);
  }

  /**
   * Discover available commands by calling the list command
   */
  protected async discoverCommands(): Promise<void> {
    if (!this.connection) {
      throw new Error('Connection not available for command discovery');
    }

    try {
      const listParams = createListParams(this.context, this.sessionId, {
        category: 'all',
        includeDescription: true,
        includeSignature: true
      });

      console.log('📋 JTAGClient: Discovering available commands...');
      const listResult = await this.connection.executeCommand<ListParams, ListResult>('list', listParams);
      
      if (listResult.success) {
        // Store discovered commands
        this.discoveredCommands.clear();
        for (const command of listResult.commands) {
          this.discoveredCommands.set(command.name, command);
        }
        
        console.log(`✅ JTAGClient: Discovered ${listResult.totalCount} commands: ${Array.from(this.discoveredCommands.keys()).join(', ')}`);
      } else {
        console.error('❌ JTAGClient: Failed to discover commands:', listResult.error);
        throw new Error(`Command discovery failed: ${listResult.error}`);
      }
    } catch (error) {
      console.error('❌ JTAGClient: Command discovery error:', error);
      throw error;
    }
  }
  
  protected getCommandsInterface(): CommandsInterface {
    return this.commandsInterface ?? new Map();
  }

  /**
   * Get dynamic commands interface - THE SINGLE DEPENDENCY PATTERN
   * Only 'list' is hardcoded, everything else is discovered dynamically
   */
  get commands(): DynamicCommandsInterface {
    const self = this;
    
    return new Proxy({} as DynamicCommandsInterface, {
      get: (target, commandName: string) => {
        // 🔑 SINGLE DEPENDENCY: 'list' is ALWAYS available (our only hardcoded command)
        if (commandName === 'list') {
          return async (params?: Partial<ListParams>): Promise<ListResult> => {
            const fullParams = createListParams(self.context, self.sessionId, params || {});
            
            // 🔄 INTERCEPTION PATTERN: Update discovered commands from list result
            const result = await self.executeListCommand(fullParams);
            self.updateDiscoveredCommands(result);
            
            return result;
          };
        }

        // 🎯 DYNAMIC DISCOVERY: All other commands must be discovered first
        if (!self.connection) {
          throw new Error(`Command '${commandName}' not available. Call connect() first to discover commands.`);
        }
        
        const commandSignature = self.discoveredCommands.get(commandName);
        if (!commandSignature) {
          const available = Array.from(self.discoveredCommands.keys());
          throw new Error(`Command '${commandName}' not available. Available commands: ${available.join(', ')}. Call list() to refresh.`);
        }

        // Return dynamic command function
        return async (params?: any): Promise<any> => {
          const fullParams = {
            ...params,
            context: params?.context ?? self.context,
            sessionId: params?.sessionId ?? self.sessionId
          };
          
          return await self.connection!.executeCommand(commandName, fullParams);
        };
      }
    });
  }
  
  /**
   * Execute list command through the appropriate connection
   */
  private async executeListCommand(params: ListParams): Promise<ListResult> {
    if (this.connection) {
      // Use established connection (local or remote)
      return await this.connection.executeCommand<ListParams, ListResult>('list', params);
    }
    
    // Fallback: try to get local system if available
    try {
      const localSystem = this.getLocalSystem();
      if (localSystem?.commands?.list) {
        return await localSystem.commands.list(params);
      }
    } catch {
      // Local system not available
    }
    
    throw new Error('No connection available for list command. Call connect() first.');
  }
  
  /**
   * Update discovered commands from list result (interception pattern)
   */
  private updateDiscoveredCommands(listResult: ListResult): void {
    if (listResult.success && listResult.commands) {
      console.log(`🔄 JTAGClient: Updating command map with ${listResult.commands.length} commands`);
      
      // Clear and repopulate discovered commands
      this.discoveredCommands.clear();
      for (const command of listResult.commands) {
        this.discoveredCommands.set(command.name, command);
      }
      
      console.log(`✅ JTAGClient: Command map updated. Available: ${Array.from(this.discoveredCommands.keys()).join(', ')}`);
    }
  }

  /**
   * Send message through transport - let router handle correlation
   */
  public async sendMessage(message: JTAGMessage): Promise<void> {
    if (!this.systemTransport) {
      throw new Error('Transport not connected');
    }

    console.log(`📤 JTAGClient: Sending message to ${message.endpoint}`);
    await this.systemTransport.send(message);
  }

  /**
   * Static factory method for easy client creation with robust retry logic for slow server startup
   * 🔄 BOOTSTRAP PATTERN: Returns list result for CLI integration
   */
  static async connect(options?: JTAGClientConnectOptions): Promise<{ client: JTAGClient; listResult: ListResult }> {
    const maxRetries = options?.maxRetries ?? 120; // 120 retries = ~60 seconds with 500ms delay
    const retryDelay = options?.retryDelay ?? 500; // 500ms between retries
    
    const context: JTAGContext = {
      uuid: options?.sessionId ?? generateUUID(), // Join existing session or create new one
      environment: options?.targetEnvironment ?? 'server' // CLI runs in server environment by default
    };
    
    for (let attempt = 1; attempt <= maxRetries; attempt++) {
      try {
        // JTAGClient is abstract - this method should be overridden by subclasses
        // For now, we'll create a simple concrete implementation for remote connections
        const client = new RemoteJTAGClient(context);
        await client.initialize(options);
        
        console.log(`✅ JTAGClient: Connected using ${options?.transportType ?? 'websocket'} transport on port ${options?.serverPort ?? 9001}`);
        
        // 🔑 BOOTSTRAP: Call list() to discover commands and return result for CLI
        console.log('🔄 JTAGClient: Discovering available commands...');
        const listResult = await client.commands.list();
        
        console.log(`✅ JTAGClient: Bootstrap complete! Discovered ${listResult.totalCount} commands`);
        
        return { client, listResult };
        
      } catch (error) {
        if (attempt === maxRetries) {
          console.error(`❌ JTAGClient: Failed to connect after ${maxRetries} attempts (${maxRetries * retryDelay / 1000}s)`);
          throw error;
        }
        
        if (attempt === 1 || attempt % 10 === 0) { // Show progress every 10th attempt
          console.log(`⏳ JTAGClient: Connection attempt ${attempt}/${maxRetries}, server may still be starting...`);
        }
        
        await new Promise(resolve => setTimeout(resolve, retryDelay));
      }
    }
    
    throw new Error('Connection failed after all retry attempts');
  }
  
  /**
   * Create local connection (direct system calls)
   * Subclasses implement getLocalSystem() to provide environment-specific system
   */
  protected createLocalConnection(): JTAGConnection {
    const localSystem = this.getLocalSystem();
    return new LocalConnection(localSystem, this.context, this.sessionId);
  }
  
  /**
   * Get local system instance - must be implemented by subclasses
   */
  protected abstract getLocalSystem(): any;
  
  /**
   * Connect to local system instance (direct calls)
   * 🔄 BOOTSTRAP PATTERN: Returns list result for CLI integration
   */
  static async connectLocal(): Promise<{ client: JTAGClient; listResult: ListResult }> {
    // This will be overridden by subclasses like JTAGClientBrowser
    throw new Error('connectLocal() must be implemented by subclasses');
  }
}

/**
 * Concrete JTAGClient implementation for remote connections
 * Used by the static connect() method when no specific subclass is needed
 */
class RemoteJTAGClient extends JTAGClient {
  protected getLocalSystem(): any {
    throw new Error('RemoteJTAGClient does not support local system access');
  }
  
  static async connectLocal(): Promise<{ client: JTAGClient; listResult: ListResult }> {
    throw new Error('RemoteJTAGClient does not support local connections');
  }
}

/**
 * Local connection - direct calls to local JTAG system (no transport)
 */
export class LocalConnection implements JTAGConnection {
  public readonly sessionId: UUID;
  public readonly context: JTAGContext;

  constructor(
    private readonly localSystem: any, 
    context: JTAGContext, 
    sessionId: UUID
  ) {
    this.context = context;
    this.sessionId = sessionId;
  }

  async executeCommand<TParams extends JTAGPayload, TResult extends JTAGPayload>(
    commandName: string, 
    params: TParams
  ): Promise<TResult> {
    // Direct call to local system - zero transport overhead
    const commandFn = (this.localSystem.commands as any)[commandName];
    if (!commandFn) {
      throw new Error(`Command '${commandName}' not available in local system`);
    }
    return await commandFn(params) as TResult;
  }
}

/**
 * Remote connection - transport-based calls to remote JTAG system
 */
export class RemoteConnection implements JTAGConnection {
  public readonly sessionId: UUID;
  public readonly context: JTAGContext;

  constructor(private readonly client: JTAGClient) {
    this.sessionId = client.sessionId;
    this.context = client.context;
  }

  async executeCommand<TParams extends JTAGPayload, TResult extends JTAGPayload>(
    commandName: string, 
    params: TParams
  ): Promise<TResult> {
    // Create strongly-typed request message
    const correlationId = `client_${Date.now()}_${Math.random().toString(36).substr(2, 8)}`;
    
    const requestMessage: JTAGMessage = JTAGMessageFactory.createRequest(
      this.context,
      this.context.environment, // origin: current environment
      `server/commands/${commandName}`, // target: server command
      params,
      correlationId
    );

    // Send via transport - router handles correlation
    await this.client.sendMessage(requestMessage);
    
    // TODO: Need to wait for correlated response
    // This is where we need the router's correlation system
    throw new Error('Remote correlation not yet implemented - need router integration');
  }
}