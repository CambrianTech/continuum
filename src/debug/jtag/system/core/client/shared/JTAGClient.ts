// ISSUES: 9 critical, last updated 2025-07-31 - ARCHITECTURAL CLEANUP REQUIRED

/**
 * JTAGClient - Universal Location-Transparent Interface to JTAG Systems
 *
 * MISSION: Provide identical `jtag.commands.screenshot()` API whether system runs 
 * locally, remotely, or on Mars. Pure location transparency through elegant abstraction.
 * Test change for smart build system validation.
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
 *    - Should be: executeCommand(name: string, params:any): Promise<any>
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
 * 🚨 ISSUE 9: TEMPORARY HARD-CODED LOCAL CONNECTIONS
 *    - Currently hard-coded to always prefer local system
 *    - Need proper detection logic for local vs remote modes
 *    - Need configuration-based connection mode selection
 *    - Need automatic fallback when local system unavailable
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


import { generateUUID, type UUID} from '../../types/CrossPlatformUUID';
import { JTAGBase, type CommandsInterface } from '../../shared/JTAGBase';
import type { JTAGContext, JTAGMessage, JTAGPayload, JTAGEnvironment } from '../../types/JTAGTypes';
import { JTAGMessageFactory, JTAGMessageTypes } from '../../types/JTAGTypes';
import { ResponseCorrelator } from '../../shared/ResponseCorrelator';
import type { ITransportFactory, TransportConfig, JTAGTransport, TransportProtocol, TransportSendResult } from '../../../transports';
import type { ITransportHandler } from '../../../transports';
import type { ListParams, ListResult, CommandSignature } from '../../../../commands/list/shared/ListTypes';
import { createListParams } from '../../../../commands/list/shared/ListTypes';
import type { BaseResponsePayload, JTAGResponsePayload } from '../../types/ResponseTypes';
import type { JTAGSystem } from '../../system/shared/JTAGSystem';
import { SYSTEM_SCOPES } from '../../types/SystemScopes';
import type { SessionMetadata } from '../../../../daemons/session-daemon/shared/SessionTypes';
import type { SessionCreateResult } from '../../../../commands/session/create/shared/SessionCreateTypes';
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
 * Connection result metadata - describes HOW the client connected
 */
export interface JTAGClientConnectionResult {
  client: JTAGClient;
  connectionType: 'local' | 'remote';
  sessionId: UUID;
  reason: string;
  localSystemAvailable: boolean;
  listResult: ListResult;
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

// TODO: Keep DynamicCommandsInterface for now - needed for remote command discovery (ISSUE 1)
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
export abstract class JTAGClient extends JTAGBase implements ITransportHandler {
  protected systemTransport?: JTAGTransport;
  protected connection?: JTAGConnection;
  // TODO: Remove discoveredCommands - redundant with CommandsInterface (ISSUE 2)
  protected discoveredCommands: Map<string, CommandSignature> = new Map();
  protected systemInstance?: JTAGSystem;
  protected responseCorrelator: ResponseCorrelator = new ResponseCorrelator(30000);
  
  // ITransportHandler implementation
  public readonly transportId: UUID;

  // Connection metadata for diagnostics
  protected connectionMetadata: {
    connectionType: 'local' | 'remote';
    reason: string;
    localSystemAvailable: boolean;
  } = {
    connectionType: 'remote',
    reason: 'Not yet initialized',
    localSystemAvailable: false
  };

  public get sessionId(): UUID {
    return this._session?.sessionId ?? SYSTEM_SCOPES.UNKNOWN_SESSION;
  }
  private _session: SessionMetadata | undefined;

  constructor(context: JTAGContext) {
    super('jtag-client', context);
    this.transportId = generateUUID();
  }
  
  /**
   * Get connection information for diagnostics and testing
   */
  public getConnectionInfo(): {
    connectionType: 'local' | 'remote';
    reason: string;
    localSystemAvailable: boolean;
    sessionId: UUID;
    environment: JTAGEnvironment;
    isBootstrapSession: boolean;
  } {
    return {
      connectionType: this.connectionMetadata.connectionType,
      reason: this.connectionMetadata.reason,
      localSystemAvailable: this.connectionMetadata.localSystemAvailable,
      sessionId: this.sessionId,
      environment: this.context.environment,
      isBootstrapSession: this.sessionId === 'deadbeef-cafe-4bad-8ace-5e551000c0de'
    };
  }


  /**
   * Handle transport messages - route responses to correlation system
   */
  async handleTransportMessage(message: JTAGMessage): Promise<JTAGResponsePayload> {
    console.log(`📥 JTAGClient: Transport message received (type: ${message.messageType})`);
    
    // Handle correlated responses - complete pending requests
    if (JTAGMessageTypes.isResponse(message)) {
      console.log(`🔗 JTAGClient: Processing response for correlation ${message.correlationId}`);
      
      const resolved = this.responseCorrelator.resolveRequest(message.correlationId, message.payload);
      if (resolved) {
        console.log(`✅ JTAGClient: Completed correlation ${message.correlationId}`);
      } else {
        console.warn(`⚠️ JTAGClient: No pending request for correlation ${message.correlationId}`);
      }
      
      // Return acknowledgment for transport protocol
      const response: BaseResponsePayload = {
        success: true,
        timestamp: new Date().toISOString(),
        context: this.context,
        sessionId: this.sessionId
      };
      return response as JTAGResponsePayload;
    }
    
    // Handle other transport protocol messages (health checks, events, etc.)
    console.log(`📋 JTAGClient: Non-response message type '${message.messageType}' - acknowledging`);
    const response: BaseResponsePayload = {
      success: true,
      timestamp: new Date().toISOString(),
      context: this.context,
      sessionId: this.sessionId
    };
    return response as JTAGResponsePayload;
  }
  

  // FIXED: Now async - may need to start system (ISSUE 4 RESOLVED)
  protected abstract getLocalSystem(): Promise<JTAGSystem | null>;

  /**
   * Abstract initialization method - subclasses implement environment-specific setup
   * 
   * TODO: Implement proper local vs remote detection logic
   * TODO: Add configuration-based connection mode selection
   * TODO: Add automatic fallback when local system unavailable
   */
  protected async initialize(options?: JTAGClientConnectOptions): Promise<void> {
    // Try local system first if available
    const localSystem = await this.getLocalSystem();
    this.connectionMetadata.localSystemAvailable = !!localSystem;
    
    if (localSystem) {
      console.log('🏠 JTAGClient: Using local system connection');
      this.systemInstance = localSystem;
      this.connectionMetadata.connectionType = 'local';
      this.connectionMetadata.reason = 'Local system instance available';
      
      this.connection = this.createLocalConnection();
    } else {
      // Remote connection setup
      this.connectionMetadata.connectionType = 'remote';
      this.connectionMetadata.reason = 'No local system available - using transport';
      
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
      this.connection = this.createRemoteConnection();
    }

    // Universal session management - works for both local and remote
    console.log('🏷️ JTAGClient: Requesting session from SessionDaemon...');

    const sessionParams = {
      context: this.context,
      sessionId: this.sessionId,
      category: 'user' as const,
      displayName: 'Anonymous User',
      isShared: true
    };
    const result = await this.connection.executeCommand('session/create', sessionParams);
    const sessionResult = result as SessionCreateResult;
    const session = sessionResult.session;
    const error = sessionResult.error;

    if (error) {
      console.error('❌ JTAGClient: Failed to create session:', error);
    } else if (session) {
      const wasBootstrap = this.sessionId === SYSTEM_SCOPES.UNKNOWN_SESSION;
      console.log(`🔄 JTAGClient: ${wasBootstrap ? 'Bootstrap complete' : 'Session updated'}: ${this.sessionId} → ${session.sessionId}`);
      this._session = session;
      
      // For browser clients: update sessionStorage (JTAGClientBrowser overrides this)
      this.updateClientSessionStorage(session.sessionId);
    }

    await this.discoverCommands();
  }

  /**
   * Get environment-specific transport factory - implemented by JT`AGClientServer/JTAGClientBrowser
   */
  protected abstract getTransportFactory(): Promise<ITransportFactory>;

  /**
   * Update client-specific session storage - overridden by JTAGClientBrowser for sessionStorage
   */
  protected updateClientSessionStorage(sessionId: UUID): void {
    // Base implementation: no-op (JTAGClientServer doesn't need session storage)
    // JTAGClientBrowser overrides this to update sessionStorage
  }

  /**
   * Get environment-specific command correlator - implemented by subclasses
   */
  protected abstract getCommandCorrelator(): ICommandCorrelator;


  /**
   * Set up commands interface that routes commands through transport to remote JTAGSystem
   * Now includes dynamic command discovery via list command
   */
  protected createRemoteConnection(): JTAGConnection {
    const correlator = this.getCommandCorrelator();
    return new RemoteConnection(this, correlator);
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

  // TODO: Keep custom commands getter for now - needed for command discovery (ISSUE 1)
  /**
   * Get dynamic commands interface - works for both local and remote
   * Local: Delegates to local system after discovery
   * Remote: Routes through transport after discovery
   */
  get commands(): DynamicCommandsInterface {
    const self = this;
    
    return new Proxy({} as DynamicCommandsInterface, {
      get: (target, commandName: string) => {
        // 'list' is always available and needed for command discovery
        if (commandName === 'list') {
          return async (params?: Partial<ListParams>): Promise<ListResult> => {
            const fullParams = createListParams(self.context, self.sessionId, params || {});
            return await self.connection!.executeCommand<ListParams, ListResult>('list', fullParams);
          };
        }
        
        // For other commands, ensure they were discovered first
        if (!self.connection) {
          throw new Error(`Command '${commandName}' not available. Call connect() first.`);
        }
        
        const commandSignature = self.discoveredCommands.get(commandName);
        if (!commandSignature) {
          const available = Array.from(self.discoveredCommands.keys());
          throw new Error(`Command '${commandName}' not available. Available commands: ${available.join(', ')}`);
        }

        // Delegate to connection (works for both local and remote)
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
  
  // TODO: Keep updateDiscoveredCommands - needed for remote command discovery (ISSUE 2)
  /**
   * Update discovered commands from list result (called during connect)
   */
  private updateDiscoveredCommands(listResult: ListResult): void {
    if (listResult.success && listResult.commands) {
      console.log(`🔍 JTAGClient: Discovered ${listResult.commands.length} commands from list response`);
      
      for (const command of listResult.commands) {
        this.discoveredCommands.set(command.name, command);
      }
    }
  }
  // REMOVED: Duplicate updateDiscoveredCommands method cleaned up

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
   * Get system transport - for use by connection classes
   */
  public getSystemTransport(): JTAGTransport | undefined {
    return this.systemTransport;
  }

  // TODO: Move static connect to subclasses - belongs on concrete classes (ISSUE 6)
  /**
   * Shared connect logic - creates client instance and bootstraps with list command
   * 🔄 BOOTSTRAP PATTERN: Returns list result for CLI integration
   */
  static async connect<T extends JTAGClient>(this: new (context: JTAGContext) => T, options?: JTAGClientConnectOptions): Promise<JTAGClientConnectionResult & { client: T }> {
    // Create context with proper system UUID (separate from sessionId)
    const context: JTAGContext = {
      uuid: generateUUID(), // System context identifier - separate from session
      environment: options?.targetEnvironment ?? 'server'
    };

    console.log(`🔄 JTAGClient: Connecting to ${context.environment} system with UNKNOWN_SESSION bootstrap...`);
    
    const client = new this(context);
    await client.initialize(options);
    
    console.log('✅ JTAGClient: Connection established');
    
    // 🔑 BOOTSTRAP: Call list() to discover commands and return result for CLI
    console.log('🔄 JTAGClient: Discovering available commands...');
    const listResult = await client.commands.list();
    
    console.log(`✅ JTAGClient: Bootstrap complete! Discovered ${listResult.totalCount} commands`);
    
    return { 
      client, 
      listResult,
      connectionType: client.connectionMetadata.connectionType,
      sessionId: client.sessionId,
      reason: client.connectionMetadata.reason,
      localSystemAvailable: client.connectionMetadata.localSystemAvailable
    };
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

  /**
   * Disconnect the client and cleanup resources
   */
  public async disconnect(): Promise<void> {
    console.log('🔌 JTAGClient: Disconnecting...');
    if (this.systemTransport) {
      await this.systemTransport.disconnect();
      console.log('✅ JTAGClient: Transport disconnected');
    } else {
      console.log('ℹ️ JTAGClient: No transport to disconnect (local connection)');
    }
  }
}


/**
 * Local connection - direct calls to local JTAG system (no transport)
 */
export class LocalConnection implements JTAGConnection {
  public readonly sessionId: UUID;
  public readonly context: JTAGContext;
  public readonly localSystem: JTAGSystem;

  constructor(
    localSystem: JTAGSystem, 
    context: JTAGContext, 
    sessionId: UUID
  ) {
    this.localSystem = localSystem;
    this.context = context;
    this.sessionId = sessionId;
  }

  async executeCommand<TParams extends JTAGPayload, TResult extends JTAGPayload>(
    commandName: string, 
    params: TParams
  ): Promise<TResult> {
    // Direct call to local system - zero transport overhead
    const commandFn = this.localSystem.commands[commandName];
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
 * Correlation interface for remote command execution
 * Implemented by environment-specific correlators
 */
export interface ICommandCorrelator {
  waitForResponse<TResult extends JTAGPayload>(correlationId: string, timeoutMs?: number): Promise<TResult>;
}

/**
 * Remote connection - transport-based calls to remote JTAG system
 */
export class RemoteConnection implements JTAGConnection {
  public readonly sessionId: UUID;
  public readonly context: JTAGContext;

  constructor(
    private readonly client: JTAGClient,
    private readonly correlator: ICommandCorrelator
  ) {
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

    // Send via transport - correlation handled by shared ResponseCorrelator
    const transport = this.client.getSystemTransport();
    if (!transport) {
      throw new Error('No transport available for remote command execution');
    }
    
    const sendResult: TransportSendResult = await transport.send(requestMessage);
    
    if (!sendResult.success) {
      throw new Error(`Transport failed to send command at ${sendResult.timestamp}`);
    }
    
    // Wait for correlated response using the shared correlation interface
    const response = await this.correlator.waitForResponse<TResult>(correlationId, 30000);
    return response;
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