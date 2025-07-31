// ISSUES: 8 critical, last updated 2025-07-31 - ARCHITECTURAL CLEANUP REQUIRED

/**
 * JTAGClient - Universal Location-Transparent Interface to JTAG Systems
 *
 * MISSION: Provide identical `jtag.commands.screenshot()` API whether system runs 
 * locally, remotely, or on Mars. Pure location transparency through elegant abstraction.
 *
 * CURRENT PROBLEMS - NEEDLESS COMPLEXITY:
 * 🚨 ISSUE 1: WRONG COMMANDS INTERFACE
 *    - Uses custom DynamicCommandsInterface instead of JTAGBase.commands
 *    - Should implement getCommandsInterface() like JTAGSystem does
 *    - Proxy pattern is overengineered - JTAGBase already provides .commands
 *
 * 🚨 ISSUE 2: COMMAND DISCOVERY OVERCOMPLICATED  
 *    - discoveredCommands Map is redundant
 *    - Dynamic proxy recreation is unnecessary
 *    - Should delegate to connection.getCommandsInterface() cleanly
 *
 * 🚨 ISSUE 3: DUAL COMMAND PATHS CONFUSION
 *    - executeListCommand() has fallback logic that's confusing
 *    - Should be: connection.executeCommand('list') - period
 *    - Fallback should happen at connection level, not command level
 *
 * 🚨 ISSUE 4: ASYNC METHODS NOT ASYNC
 *    - getLocalSystem() should be async (may start system)
 *    - createLocalConnection() should be async (may initialize system)
 *    - All connection setup is inherently async
 *
 * 🚨 ISSUE 5: CONNECTION ABSTRACTION LEAKY
 *    - JTAGConnection interface forces JTAGPayload generics
 *    - Should be: executeCommand(name: string, params: any): Promise<any>
 *    - Over-typing creates complexity without benefit
 *
 * 🚨 ISSUE 6: STATIC CONNECT METHOD WRONG PLACE
 *    - Static methods belong on subclasses, not abstract base
 *    - JTAGClientBrowser.connect(), JTAGClientServer.connect()
 *    - Base class should only have instance methods
 *
 * 🚨 ISSUE 7: TRANSPORT HANDLER RESPONSIBILITY CONFUSION
 *    - JTAGClient implements ITransportHandler but doesn't handle transport messages
 *    - Either implement properly or remove the interface
 *    - Mixing client and server responsibilities
 *
 * 🚨 ISSUE 8: INITIALIZATION OVERCOMPLICATED
 *    - initialize() tries to be smart about local vs remote
 *    - Should be: try getLocalSystem(), if null use transport
 *    - Decision logic should be simple and clear
 *
 * ELEGANT SOLUTION - FOLLOW JTAGSYSTEM PATTERN:
 * 
 * 1. IMPLEMENT getCommandsInterface() PROPERLY:
 *    ```typescript
 *    protected getCommandsInterface(): CommandsInterface {
 *      return this.connection?.getCommandsInterface() ?? new Map();
 *    }
 *    ```
 *
 * 2. CONNECTION PROVIDES COMMANDS:
 *    ```typescript
 *    interface JTAGConnection {
 *      getCommandsInterface(): CommandsInterface;
 *      // Remove executeCommand - commands come from interface
 *    }
 *    ```
 *
 * 3. LOCAL CONNECTION DELEGATES:
 *    ```typescript
 *    getCommandsInterface() {
 *      return this.localSystem.getCommandsInterface();
 *    }
 *    ```
 *
 * 4. REMOTE CONNECTION PROXIES:
 *    ```typescript
 *    getCommandsInterface() {
 *      return this.createRemoteCommandsInterface();
 *    }
 *    ```
 *
 * 5. CLEAN ASYNC PATTERN:
 *    ```typescript
 *    async initialize() {
 *      const localSystem = await this.getLocalSystem();
 *      this.connection = localSystem 
 *        ? await this.createLocalConnection(localSystem)
 *        : await this.createRemoteConnection();
 *    }
 *    ```
 *
 * RESULT: JTAGClient.commands works exactly like JTAGSystem.commands
 * - Same JTAGBase.commands interface
 * - Same CommandsInterface backend  
 * - Local delegates to system, remote proxies to transport
 * - Zero special cases, zero complexity
 */


import { generateUUID, type UUID} from './CrossPlatformUUID';
import { JTAGBase, type CommandsInterface } from './JTAGBase';
import type { JTAGContext, JTAGMessage, JTAGPayload, CommandParams, CommandResult } from './JTAGTypes';
import { JTAGMessageFactory } from './JTAGTypes';
import type { ITransportFactory, TransportConfig, JTAGTransport, TransportProtocol } from '@systemTransports';
import type { ITransportHandler } from '../system/transports/shared/ITransportHandler';
import type { ListParams, ListResult, CommandSignature } from '../commands/list/shared/ListTypes';
import { createListParams } from '../commands/list/shared/ListTypes';
import type { BaseResponsePayload, JTAGResponsePayload } from './ResponseTypes';
import type { JTAGSystem } from './JTAGSystem';

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

// TODO: Fix JTAGConnection interface - remove generics, add getCommandsInterface() (ISSUE 5)
/**
 * Connection abstraction - local vs remote execution strategy
 */
export interface JTAGConnection {
  executeCommand<TParams extends JTAGPayload, TResult extends JTAGPayload>(
    commandName: string, 
    params: TParams
  ): Promise<TResult>;
  
  getCommandsInterface(): CommandsInterface;
  
  readonly sessionId: UUID;
  readonly context: JTAGContext;
}

// TODO: Remove DynamicCommandsInterface - use JTAGBase.commands instead (ISSUE 1)
/**
 * Dynamic commands interface generated from server's list command
 */
export interface DynamicCommandsInterface {
  // Essential commands (always available)
  list(params?: Partial<ListParams>): Promise<ListResult>;
  
  // Dynamic commands discovered from server
  [commandName: string]: (params?: any) => Promise<any>;
}

// TODO: Remove ITransportHandler - mixing client/server responsibilities (ISSUE 7)
export abstract class JTAGClient extends JTAGBase /* implements ITransportHandler */ {
  protected systemTransport?: JTAGTransport;
  protected connection?: JTAGConnection;
  // TODO: Remove discoveredCommands - redundant with CommandsInterface (ISSUE 2)
  protected discoveredCommands: Map<string, CommandSignature> = new Map();
  protected systemInstance?: JTAGSystem;

  public readonly sessionId: UUID;

  constructor(context: JTAGContext) {
    super('jtag-client', context);
    this.sessionId = context.uuid;
  }


  // TODO: Remove handleTransportMessage - not needed for client (ISSUE 7)
  // ITransportHandler implementation - payload in, payload out
  async handleTransportMessage(message: JTAGMessage): Promise<JTAGResponsePayload> {
    console.log(`📥 JTAGClient: Transport message received:`, message);
    
    // Handle transport protocol messages (health checks, events, correlation)
    // Same pattern as daemon handleMessage methods
    
    // For now, return success response
    // TODO: Implement specific transport message handling
    const response: BaseResponsePayload = {
      success: true,
      timestamp: new Date().toISOString(),
      context: this.context,
      sessionId: this.sessionId
    };
    return response as JTAGResponsePayload;
  }
  
  get transportId(): UUID {
    return this.sessionId;
  }

  // TODO: Make async - may need to start system (ISSUE 4)
  protected abstract getLocalSystem(): JTAGSystem | null;

  /**
   * Abstract initialization method - subclasses implement environment-specific setup
   */
  protected async initialize(options?: JTAGClientConnectOptions): Promise<void> {
    // Try local system first if available
    const localSystem = await this.getLocalSystem();
    if (localSystem && !options?.transportType) {
      this.systemInstance = localSystem;
      this.connection = await this.createLocalConnection();
    } else {
      // Remote connection setup
      const transportConfig: TransportConfig = { 
        protocol: (options?.transportType ?? 'websocket') as TransportProtocol,
        role: 'client',
        eventSystem: this.eventManager.events,
        sessionId: this.sessionId,
        serverPort: options?.serverPort ?? 9001,
        serverUrl: options?.serverUrl ?? `ws://localhost:${options?.serverPort ?? 9001}`,
        fallback: options?.enableFallback ?? true,
        handler: this
      };

      const factory = await this.getTransportFactory();
      this.systemTransport = await factory.createTransport(this.context.environment, transportConfig);
      this.connection = await this.createRemoteConnection();
    }

    await this.discoverCommands();
  }

  /**
   * Get environment-specific transport factory - implemented by JTAGClientServer/JTAGClientBrowser
   */
  protected abstract getTransportFactory(): Promise<ITransportFactory>;

  /**
   * Set up commands interface that routes commands through transport to remote JTAGSystem
   * Now includes dynamic command discovery via list command
   */
  protected createRemoteConnection(): JTAGConnection {
    return new RemoteConnection(this);
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
  
  // TODO: Fix to delegate to connection.getCommandsInterface() (ISSUE 1)
  protected getCommandsInterface(): CommandsInterface {
    // Delegate to connection's CommandsInterface
    return this.connection?.getCommandsInterface() ?? new Map();
  }

  // TODO: Remove custom commands getter - use JTAGBase.commands instead (ISSUE 1)
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
  
  // TODO: Remove executeListCommand - dual command paths are confusing (ISSUE 3)
  /**
   * Execute list command through the appropriate connection
   */
  private async executeListCommand(params: ListParams): Promise<ListResult> {
    if (!this.connection) {
      throw new Error('No connection available for list command. Call connect() first.');
    }
    
    // Delegate to connection (local system has already taken care of list command)
    return await this.connection.executeCommand<ListParams, ListResult>('list', params);
  }
  
  // TODO: Remove updateDiscoveredCommands - redundant with CommandsInterface (ISSUE 2)
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

  // TODO: Move static connect to subclasses - belongs on concrete classes (ISSUE 6)
  /**
   * Shared connect logic - creates client instance and bootstraps with list command
   * 🔄 BOOTSTRAP PATTERN: Returns list result for CLI integration
   */
  static async connect<T extends JTAGClient>(this: new (context: JTAGContext) => T, options?: JTAGClientConnectOptions): Promise<{ client: T; listResult: ListResult }> {
    const context: JTAGContext = {
      uuid: options?.sessionId ?? generateUUID(),
      environment: options?.targetEnvironment ?? 'server'
    };

    console.log(`🔄 JTAGClient: Connecting to ${context.environment} system...`);
    
    const client = new this(context);
    await client.initialize(options);
    
    console.log('✅ JTAGClient: Connection established');
    
    // 🔑 BOOTSTRAP: Call list() to discover commands and return result for CLI
    console.log('🔄 JTAGClient: Discovering available commands...');
    const listResult = await client.commands.list();
    
    console.log(`✅ JTAGClient: Bootstrap complete! Discovered ${listResult.totalCount} commands`);
    
    return { client, listResult };
  }
  
  // TODO: Make async and take system parameter - connection setup is async (ISSUE 4)
  /**
   * Create local connection (direct system calls)
   */
  protected createLocalConnection(): JTAGConnection {
    if (!this.systemInstance) {
      throw new Error('Local system instance not available');
    }
    return new LocalConnection(this.systemInstance, this.context, this.sessionId);
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

  getCommandsInterface(): CommandsInterface {
    // Delegate to local system's getCommandsInterface - zero transport overhead
    return this.localSystem.getCommandsInterface();
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

  getCommandsInterface(): CommandsInterface {
    // For remote connections, we need to create a CommandsInterface Map
    // from the dynamically discovered commands
    const map = new Map();
    
    // For now, create an empty map - this will be populated after command discovery
    // TODO: Integrate with client's discovered commands system
    return map;
  }
}