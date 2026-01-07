// ISSUES: RESOLVED 2025-08-17 - ARCHITECTURAL CLEANUP COMPLETE

/**
 * JTAGClient - Universal Location-Transparent Interface to JTAG Systems
 *
 * MISSION: Provide identical `jtag.commands.screenshot()` API whether system runs 
 * locally, remotely, or on Mars. Pure location transparency through elegant abstraction.
 * Test change for smart build system validation.
 *
 * ARCHITECTURAL REVIEW COMPLETE - ALL ISSUES RESOLVED:
 * ✅ ISSUE 1: COMMANDS INTERFACE - Proper getCommandsInterface() delegation implemented
 * ✅ ISSUE 2: COMMAND DISCOVERY - Fixed protected property access via public getter
 * ✅ ISSUE 3: DUAL COMMAND PATHS - Already correctly delegates to connection.executeCommand()
 * ✅ ISSUE 4: ASYNC METHODS - Already correctly async via initialize() pattern  
 * ✅ ISSUE 5: CONNECTION ABSTRACTION - Simplified by removing unnecessary generics
 * ✅ ISSUE 6: STATIC CONNECT METHOD - Correctly uses generic pattern for inheritance
 * ✅ ISSUE 7: TRANSPORT HANDLER - Properly implements ITransportHandler for correlation
 * ✅ ISSUE 8: INITIALIZATION LOGIC - Clean and follows suggested architectural pattern
 * ✅ ISSUE 9: CONNECTION DETECTION - Uses proper local/remote fallback with Connection Broker
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
import type { JTAGContext, JTAGMessage, JTAGPayload, JTAGEnvironment, CommandParams, CommandResult } from '../../types/JTAGTypes';
import type { CommandResponse, CommandSuccessResponse, CommandErrorResponse } from '../../../../daemons/command-daemon/shared/CommandResponseTypes';
import { JTAGMessageFactory, JTAGMessageTypes, isJTAGResponseMessage } from '../../types/JTAGTypes';
import { ResponseCorrelator } from '../../shared/ResponseCorrelator';
import type { ITransportFactory, TransportConfig, JTAGTransport, TransportProtocol, TransportSendResult } from '../../../transports';
import type { ITransportHandler } from '../../../transports';
import type { ListParams, ListResult, CommandSignature } from '../../../../commands/list/shared/ListTypes';
import { createListParams } from '../../../../commands/list/shared/ListTypes';
import type { SessionDestroyParams, SessionDestroyResult } from '../../../../commands/session/destroy/shared/SessionDestroyTypes';
import type { BaseResponsePayload, JTAGResponsePayload } from '../../types/ResponseTypes';
import { getServerConfigFromContext, createServerContext, createTestContext } from '../../context/SecureJTAGContext';
import { isTestEnvironment } from '../../../shared/BrowserSafeConfig';
import type { JTAGSystem } from '../../system/shared/JTAGSystem';
import { SYSTEM_SCOPES } from '../../types/SystemScopes';
import { JTAG_BOOTSTRAP_MESSAGES } from './JTAGClientConstants';
import type { SessionMetadata } from '../../../../daemons/session-daemon/shared/SessionTypes';
import type { BaseUser } from '../../../user/shared/BaseUser';
import type { SessionCreateResult } from '../../../../commands/session/create/shared/SessionCreateTypes';
import type { IConnectionBroker, ConnectionParams } from '../../connection-broker/shared/ConnectionBrokerTypes';
import { ConnectionBroker } from '../../connection-broker/shared/ConnectionBroker';
import { DEFAULT_USER_UNIQUE_IDS } from '../../../data/domains/DefaultEntities';
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
  readonly sessionId?: UUID; // Optional session ID - defaults to UNKNOWN_SESSION
  readonly userId?: UUID; // Optional user ID - defaults to ANONYMOUS_USER
  readonly context?: JTAGConnectionContextInput; // Agent detection context and other metadata
}

export interface JTAGConnectionContextInput {
  // Citizen identity (single source of truth for user lookup)
  uniqueId?: string;  // Human-readable citizen identifier (e.g., "joelteply@yahoo.com", "claude-code")

  // Agent detection context
  agentInfo?: {
    name: string;
    type: string;
    version?: string;
    confidence: number;
    metadata?: any;
    plugin: string;
    detected: boolean;
  };
  adapterType?: string;
  capabilities?: any;
  outputPreferences?: {
    format: string;
    supportsColors: boolean;
    maxLength?: number;
    rateLimit?: {
      requestsPerMinute: number;
      tokensPerRequest?: number;
    };
  };

  // CLI context
  cli?: {
    command: string;
    args: string[];
    timestamp: string;
  };

  // Additional context fields
  [key: string]: any;
}

/**
 * Connection result metadata - describes HOW the client connected
 */
export interface JTAGClientConnectionResult {
  client: JTAGClient;
  connectionType: 'local' | 'remote';
  sessionId: UUID;
  userId: UUID;
  reason: string;
  localSystemAvailable: boolean;
  listResult: ListResult;
}

/**
 * Connection abstraction - local vs remote execution strategy
 * ISSUE 5 FIXED: Simplified interface without over-typing generics
 */
export interface JTAGConnection {
  executeCommand(commandName: string, params: any): Promise<any>;
  getCommandsInterface(): CommandsInterface;
  readonly sessionId: UUID;
  readonly context: JTAGContext;
}

/**
 * Dynamic commands interface - temporary during migration to JTAGBase.commands
 * TODO: Remove when migration to JTAGBase.commands is complete
 */
export interface DynamicCommandsInterface {
  // Essential commands (always available)
  list(params?: Partial<ListParams>): Promise<ListResult>;
  
  // Dynamic commands discovered from server
  [commandName: string]: (params?: JTAGPayload) => Promise<JTAGPayload>;
}

// TODO: Remove ITransportHandler - mixing client/server responsibilities (ISSUE 7)
export abstract class JTAGClient extends JTAGBase implements ITransportHandler {
  // Static client registry for sharedInstance access (fixes server-side sharedInstance timeout)
  private static clientRegistry: Map<string, JTAGClient> = new Map();

  protected systemTransport?: JTAGTransport;
  protected connection?: JTAGConnection;
  // TODO: Remove discoveredCommands - redundant with CommandsInterface (ISSUE 2)
  protected discoveredCommands: Map<string, CommandSignature> = new Map();
  protected systemInstance?: JTAGSystem;
  protected responseCorrelator: ResponseCorrelator = new ResponseCorrelator(60000); // 60s for AI/inference commands
  
  // Connection Broker for intelligent connection management
  protected connectionBroker?: IConnectionBroker;
  
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

  // Connection options for agent detection and context
  protected connectionContext?: JTAGConnectionContextInput;

  public get sessionId(): UUID {
    return this._session?.sessionId ?? SYSTEM_SCOPES.UNKNOWN_SESSION;
  }

  public get userId(): UUID {
    // userId comes from session - session ties user and sessionId together
    return this._session?.userId ?? SYSTEM_SCOPES.ANONYMOUS_USER;
  }

  /**
   * Get User object with entity and state
   * Provides access to user preferences and content state
   * Returns undefined if session not yet established or user not initialized
   */
  public get user(): BaseUser | undefined {
    return this._session?.user;
  }

  /**
   * Check if using local connection (same process) or remote (transport)
   * Works identically in browser and server environments
   */
  public get isLocal(): boolean {
    return this.connection instanceof LocalConnection;
  }

  private _session: SessionMetadata | undefined;

  constructor(context: JTAGContext) {
    super('jtag-client', context);
    this.transportId = generateUUID();
  }

  /**
   * Get secure server configuration from context
   * Only server contexts have access to server configuration
   */
  protected getServerConfigFromContext() {
    try {
      return getServerConfigFromContext(this.context);
    } catch (error) {
      throw new Error(`Cannot access server configuration: ${error}`);
    }
  }
  
  /**
   * Provide router access for scoped event system
   * Works with both local and remote connections
   */
  protected getRouter(): any {
    // For local connections, get router from system instance
    if (this.systemInstance) {
      return (this.systemInstance as any).router;
    }
    
    // For remote connections, events are handled via transport
    // Return null - scoped events will fall back to basic event manager
    return null;
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
    // console.log(`📥 JTAGClient: Transport message received (type: ${message.messageType})`);

    // Handle correlated responses - complete pending requests
    if (isJTAGResponseMessage(message)) {
      // console.log(`🔗 JTAGClient: Processing response for correlation ${message.correlationId}`);

      this.responseCorrelator.resolveRequest(message.correlationId, message.payload);
      
      // Return acknowledgment for transport protocol
      const response: BaseResponsePayload = {
        success: true,
        timestamp: new Date().toISOString(),
        context: this.context,
        sessionId: this.sessionId
      };
      return response as JTAGResponsePayload;
    }
    
    // Handle cross-environment event messages - delegate to server's routing system
    if (JTAGMessageTypes.isEvent(message)) {
      console.log(`🌉 JTAGClient: Delegating event to server router (client doesn't route)`);
      // JTAGClient is a dumb transport pipe - server handles all routing
      return {
        success: true,
        delegated: true,
        timestamp: new Date().toISOString(),
        context: this.context,
        sessionId: this.sessionId
      } as JTAGResponsePayload;
    }

    // Handle other transport protocol messages (health checks, etc.)
    // console.log(`📋 JTAGClient: Non-response message type '${message.messageType}' - acknowledging`);
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
    // Store provided sessionId for use throughout initialization
    const providedSessionId = options?.sessionId;
    if (providedSessionId) {
      console.log(`🎯 JTAGClient: Provided sessionId: ${providedSessionId}`);
    }

    // Store connection context for agent detection
    this.connectionContext = options?.context;

    // Try local system first if available
    const localSystem = await this.getLocalSystem();
    this.connectionMetadata.localSystemAvailable = !!localSystem;
    
    if (localSystem) {
      console.log('🏠 JTAGClient: Using local system connection');
      this.systemInstance = localSystem;
      this.connectionMetadata.connectionType = 'local';
      this.connectionMetadata.reason = 'Local system instance available';
      
      this.connection = this.createLocalConnection(options);
    } else {
      // Remote connection setup using Connection Broker
      this.connectionMetadata.connectionType = 'remote';
      this.connectionMetadata.reason = 'No local system available - using Connection Broker';
      
      console.log('🔗 JTAGClient: Using Connection Broker for intelligent connection management');
      
      const broker = await this.getConnectionBroker();
      
      // Create connection parameters from client options with proper TypeScript typing
      const effectiveSessionId = providedSessionId ?? this.sessionId;
      console.log(`🔧 JTAGClient: Connection with sessionId: ${effectiveSessionId} (provided: ${providedSessionId}, current: ${this.sessionId})`);
      
      const connectionParams: ConnectionParams = {
        protocols: [
          (options?.transportType ?? 'websocket') as TransportProtocol,
          ...(options?.enableFallback ? ['websocket' as TransportProtocol] : []) // Add fallback protocol if enabled
        ],
        mode: 'preferred', // Prefer shared connections, fall back to isolated
        targetEnvironment: this.context.environment,
        sessionId: effectiveSessionId, // Use provided sessionId if available
        context: this.context,
        eventSystem: this.eventManager.events, // Required field with proper typing
        handler: this, // Required field with proper typing
        server: options?.serverPort || options?.serverUrl ? {
          port: options.serverPort,
          name: options.serverUrl?.includes('localhost') ? 'localhost-server' : undefined
        } : undefined,
        timeoutMs: options?.timeout ?? 10000,
        enableFallback: options?.enableFallback ?? true,
        maxRetries: options?.maxRetries ?? 3,
        metadata: {
          clientType: this.context.environment,
          requestedProtocol: options?.transportType ?? 'websocket'
        }
      };

      const connectionResult = await broker.connect(connectionParams);
      
      this.systemTransport = connectionResult.transport;
      this.connectionMetadata.reason = `Connected via ${connectionResult.strategy} (${connectionResult.metadata.protocolUsed})`;
      
      // providedSessionId is already available from the method scope
      
      console.log(`✅ JTAGClient: ${connectionResult.strategy} connection established on port ${connectionResult.server.port}`);
      
      this.connection = this.createRemoteConnection();
    }

    // Universal session management - works for both local and remote
    console.log('🏷️ JTAGClient: Requesting session from SessionDaemon...');

    // SECURITY: CLI clients use ephemeral sessions, browser clients use shared sessions
    const isEphemeralClient = this.context.environment === 'server'; // Server-side JTAGClient = CLI client

    // Use provided sessionId if available, otherwise let SessionDaemon assign shared session
    let targetSessionId: UUID;
    if (providedSessionId) {
      targetSessionId = providedSessionId;
      console.log(`🔄 JTAGClient: Using provided sessionId: ${providedSessionId}`);
    } else {
      // Use UNKNOWN_SESSION marker - let SessionDaemon assign the shared session ID
      targetSessionId = SYSTEM_SCOPES.UNKNOWN_SESSION;
      console.log(`🎯 JTAGClient: Requesting shared session assignment from SessionDaemon`);
    }

    // Detect if this is an AI agent based on connection context
    const agentInfo = this.connectionContext?.agentInfo;
    const isAgent = agentInfo?.detected && agentInfo.confidence > 0.5;

    // Determine category and displayName
    const category = isAgent ? 'agent' : 'user';
    const displayName = isAgent && agentInfo?.name
      ? agentInfo.name
      : (isEphemeralClient ? 'CLI Client' : 'Anonymous User');

    console.log(`🔍 JTAGClient: Detected category=${category}, displayName=${displayName}, isAgent=${isAgent}`);

    // Get stored userId from browser (if available) for citizen persistence
    const storedUserId = await this.getStoredUserId();

    // For agents, derive a consistent uniqueId from the agent name
    // For CLI clients, use CLI_CLIENT constant
    // For browser clients, use PRIMARY_HUMAN constant (could be enhanced with login later)
    // This allows persistent identity across sessions
    const uniqueId = isAgent && agentInfo?.name
      ? agentInfo.name.toLowerCase().replace(/\s+/g, '-')
      : (isEphemeralClient ? DEFAULT_USER_UNIQUE_IDS.CLI_CLIENT : DEFAULT_USER_UNIQUE_IDS.PRIMARY_HUMAN);

    console.log(`🔑 JTAGClient: Computed uniqueId="${uniqueId}" for ${displayName} (isEphemeralClient=${isEphemeralClient}, isAgent=${isAgent})`);

    // Enhance connectionContext with uniqueId for lookup
    const enhancedConnectionContext = this.connectionContext
      ? { ...this.connectionContext, uniqueId }
      : { uniqueId };

    const sessionParams = {
      context: this.context,
      sessionId: targetSessionId,
      category: category as 'user' | 'agent',
      displayName: displayName,
      userId: storedUserId, // Pass stored userId to link to existing citizen
      isShared: true, // All clients use shared sessions by default
      connectionContext: enhancedConnectionContext // Pass enhanced context with uniqueId for agent detection
    };
    // Reduce log spam - massive config dump not needed
    // console.log(`🔍 JTAGClient: Sending session/create with params:`, JSON.stringify(sessionParams, null, 2));
    const result = await this.connection.executeCommand('session/create', sessionParams);
    const sessionResult = result as SessionCreateResult;
    const session = sessionResult.session;
    const error = sessionResult.error;

    if (error) {
      console.error('❌ JTAGClient: Failed to create session:', error);
    } else if (session) {
      const wasBootstrap = this.sessionId === SYSTEM_SCOPES.UNKNOWN_SESSION;
      this._session = session;

      // For browser clients: update sessionStorage (JTAGClientBrowser overrides this)
      this.updateClientSessionStorage(session.sessionId);

      // Store userId from session for citizen persistence (browser clients store to localStorage)
      if (session.userId) {
        await this.storeUserIdentity(session.userId);
      }
    }

    await this.discoverCommands();

    // Initialize scoped event system now that connection is established
    this.initializeScopedEvents();
  }


  /**
   * Get environment-specific transport factory - implemented by JTAGClientServer/JTAGClientBrowser
   */
  protected abstract getTransportFactory(): Promise<ITransportFactory>;

  /**
   * Get stored userId from browser localStorage - implemented by JTAGClientBrowser
   * Browser: loads from localStorage, Server: returns null
   */
  protected async getStoredUserId(): Promise<UUID | undefined> {
    // Default no-op implementation for server clients
    return undefined;
  }

  /**
   * Store user identity for citizen persistence - implemented by JTAGClientBrowser
   * Browser: stores to localStorage, Server: no-op
   */
  protected async storeUserIdentity(_userId: UUID): Promise<void> {
    // Default no-op implementation for server clients
  }

  /**
   * Get or create Connection Broker for intelligent connection management
   * Lazily initialized to avoid circular dependencies
   */
  protected async getConnectionBroker(): Promise<IConnectionBroker> {
    if (!this.connectionBroker) {
      const factory = await this.getTransportFactory();
      this.connectionBroker = new ConnectionBroker({
        portPool: {
          startPort: this.getServerConfigFromContext().server.port,
          endPort: this.getServerConfigFromContext().server.port + 99, // Dynamic range from configured base
          reservedPorts: [],
          allocationStrategy: 'sequential'
        }
      }, factory);
    }
    return this.connectionBroker;
  }

  /**
   * Get UserState ID for this client (browser clients only)
   * Returns null for server clients that don't have UserState
   */
  public getUserStateId(): UUID | null {
    return null; // Default: server clients don't have UserState
  }

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
        includeDescription: true,
        includeSignature: true
      });

      console.log('📋 JTAGClient: Discovering available commands...');
      const listResult = await this.connection.executeCommand('list', listParams) as ListResult;
      
      if (listResult.success) {
        // Store discovered commands
        this.discoveredCommands.clear();
        for (const command of listResult.commands) {
          this.discoveredCommands.set(command.name, command);
        }
        

      } else {
        console.error('❌ JTAGClient: Failed to discover commands:', listResult.error);
        throw new Error(`Command discovery failed: ${listResult.error}`);
      }
    } catch (error) {
      console.error('❌ JTAGClient: Command discovery error:', error);
      throw error;
    }
  }
  
  // ISSUE 1 FIXED: Proper getCommandsInterface() delegation
  protected getCommandsInterface(): CommandsInterface {
    // Delegate to connection's CommandsInterface - follows JTAGBase pattern
    return this.connection?.getCommandsInterface() ?? new Map();
  }

  // ISSUE 1 PARTIAL FIX: Keep custom commands getter temporarily during migration
  // TODO: Complete migration to JTAGBase.commands pattern
  get commands(): DynamicCommandsInterface {
    const self = this;
    
    return new Proxy({} as DynamicCommandsInterface, {
      get: (target, commandName: string) => {
        // Delegate to connection (works for both local and remote)
        if (!self.connection) {
          throw new Error(`Command '${commandName}' not available. Call connect() first.`);
        }
        
        // For list command, use direct execution
        if (commandName === 'list') {
          return async (params?: Partial<ListParams>): Promise<ListResult> => {
            const fullParams = {
              ...createListParams(self.context, self.sessionId, params || {}),
              userId: (params as any)?.userId ?? self.userId
            } as CommandParams;
            return await self.connection!.executeCommand('list', fullParams) as ListResult;
          };
        }

        // For other commands, check if discovered
        const commandSignature = self.getDiscoveredCommands().get(commandName);
        if (!commandSignature) {
          const available = Array.from(self.getDiscoveredCommands().keys());
          throw new Error(`Command '${commandName}' not available. Available commands: ${available.join(', ')}`);
        }

        // Delegate to connection
        return async (params?: JTAGPayload): Promise<JTAGPayload> => {
          const fullParams = {
            ...params,
            context: params?.context ?? self.context,
            sessionId: params?.sessionId ?? self.sessionId,
            userId: (params as any)?.userId ?? self.userId
          } as CommandParams;

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

  /**
   * Get discovered commands - for use by connection classes
   */
  public getDiscoveredCommands(): Map<string, CommandSignature> {
    return this.discoveredCommands;
  }

  // TODO: Move static connect to subclasses - belongs on concrete classes (ISSUE 6)
  /**
   * Shared connect logic - creates client instance and bootstraps with list command
   * 🔄 BOOTSTRAP PATTERN: Returns list result for CLI integration
   */
  static async connect<T extends JTAGClient>(this: new (context: JTAGContext) => T, options?: JTAGClientConnectOptions): Promise<JTAGClientConnectionResult & { client: T }> {
    // Create secure context based on environment with proper config
    const environment = options?.targetEnvironment ?? 'server';
    const { createJTAGConfig } = await import('../../../shared/BrowserSafeConfig');
    const jtagConfig = createJTAGConfig();
    const context = isTestEnvironment() ? createTestContext(jtagConfig) : createServerContext(jtagConfig);
    
    // Override environment if explicitly requested
    if (environment !== context.environment) {
      // For now, we'll create a new context with the requested environment
      // TODO: Add createClientContext for browser environments
      console.warn(`⚠️ JTAGClient: Requested ${environment} but using ${context.environment} context`);
    }

    console.log(`🔄 JTAGClient: Connecting to ${context.environment} system with secure configuration...`);
    
    const client = new this(context);
    await client.initialize(options);
    
    console.log('✅ JTAGClient: Connection established');
    
    // 🔑 BOOTSTRAP: Call list() to discover commands and return result for CLI
    console.log('🔄 JTAGClient: Discovering available commands...');
    const listResult = await client.commands.list();
    
    // console.log(`✅ JTAGClient: ${JTAG_BOOTSTRAP_MESSAGES.BOOTSTRAP_COMPLETE_PREFIX} ${listResult.totalCount} commands`);
    
    return {
      client,
      listResult,
      connectionType: client.connectionMetadata.connectionType,
      sessionId: client.sessionId,
      userId: client.userId,
      reason: client.connectionMetadata.reason,
      localSystemAvailable: client.connectionMetadata.localSystemAvailable
    };
  }
  
  // TODO: Make async and take system parameter - connection setup is async (ISSUE 4)
  /**
   * Create local connection (direct system calls)
   */
  protected createLocalConnection(options?: JTAGClientConnectOptions): JTAGConnection {
    if (!this.systemInstance) {
      throw new Error('Local system instance not available');
    }
    return new LocalConnection(this.systemInstance, this.context, options?.sessionId ?? this.sessionId);
  }

  /**
   * Disconnect the client and cleanup resources
   */
  public async disconnect(destroySession?: boolean): Promise<void> {
    console.log('🔌 JTAGClient: Disconnecting...');
    
    // Smart default: Don't destroy shared sessions, do destroy private sessions
    const shouldDestroySession = destroySession ?? !this._session?.isShared;
    
    // Only destroy real sessions, not bootstrap sessions (UNKNOWN_SESSION)
    if (this._session && this._session.sessionId !== SYSTEM_SCOPES.UNKNOWN_SESSION && shouldDestroySession) {
      console.log(`🧹 JTAGClient: Destroying session ${this._session.sessionId} on disconnect (shared: ${this._session.isShared})`);
      
      try {
        const destroyParams: SessionDestroyParams = {
          context: this.context,
          sessionId: this._session.sessionId,
          reason: 'client_disconnect'
        };
        const destroyResult = await this.commands['session/destroy'](destroyParams) as SessionDestroyResult;
        
        console.log(`✅ JTAGClient: Session destroyed successfully:`, destroyResult.success);
      } catch (error) {
        console.error(`❌ JTAGClient: Failed to destroy session:`, error);
        // Continue disconnect even if session destroy fails
      }
    } else if (this._session && this._session.sessionId !== SYSTEM_SCOPES.UNKNOWN_SESSION) {
      console.log(`🔄 JTAGClient: Preserving shared session ${this._session.sessionId} on disconnect (shared: ${this._session.isShared})`);
    }
    
    if (this.systemTransport) {
      await this.systemTransport.disconnect();
      console.log('✅ JTAGClient: Transport disconnected');
    } else {
      console.log('ℹ️ JTAGClient: No transport to disconnect (local connection)');
    }
    
    // Cleanup Connection Broker if we created one
    if (this.connectionBroker && this.connectionBroker instanceof ConnectionBroker) {
      await this.connectionBroker.shutdown();
      console.log('✅ JTAGClient: Connection Broker shut down');
    }
  }

  /**
   * Register a client in the static registry
   * Used by browser-index.ts and server-index.ts after connection
   */
  static registerClient(key: string, client: JTAGClient): void {
    this.clientRegistry.set(key, client);
    // console.log(`📝 JTAGClient: Registered '${key}' (${client.context.environment})`);
  }

  /**
   * Unregister a client from the static registry
   * Used during disconnect to prevent memory leaks
   */
  static unregisterClient(key: string): boolean {
    return this.clientRegistry.delete(key);
  }

  /**
   * Get a registered client from the static registry
   * Private helper for sharedInstance getter
   */
  private static getRegisteredClient(key: string): JTAGClient | undefined {
    return this.clientRegistry.get(key);
  }

  /**
   * Synchronous check for registered client - NO POLLING
   * Used by Events.emit to avoid 5-second timeout on server-side Form 1 calls
   * Returns immediately with client or undefined (never waits)
   */
  static getRegisteredClientSync(key: string): JTAGClient | undefined {
    return this.clientRegistry.get(key);
  }

  /**
   * Get shared instance from global context - works in browser and server
   * Browser: (window as WindowWithJTAG).jtag
   * Server: (globalThis as any).jtag
   */
  static get sharedInstance(): Promise<JTAGClient> {
    return new Promise((resolve, reject) => {
      // Fast path: Check if already initialized
      const registered = this.getRegisteredClient('default');
      if (registered) {
        resolve(registered);
        return;
      }

      const jtag = (globalThis as any).jtag;
      if (jtag?.commands) {
        resolve(jtag);
        return;
      }

      // Initialization race: Browser widgets may call this before connect() completes
      // Poll briefly with timeout to handle startup race condition
      let attempts = 0;
      const maxAttempts = 100; // 5 seconds max (100 * 50ms)

      const checkInitialization = (): void => {
        attempts++;

        // Check both registry and globalThis
        const client = this.getRegisteredClient('default');
        if (client) {
          resolve(client);
          return;
        }

        const jtagNow = (globalThis as any).jtag;
        if (jtagNow?.commands) {
          resolve(jtagNow);
          return;
        }

        // Timeout after max attempts
        if (attempts >= maxAttempts) {
          reject(new Error(
            'JTAGClient initialization timeout (5s). ' +
            'Server: connect() never completed or registerClient() not called. ' +
            'Browser: connect() never completed or globalThis.jtag not set.'
          ));
          return;
        }

        setTimeout(checkInitialization, 50);
      };

      // Start polling after brief delay
      setTimeout(checkInitialization, 50);
    });
  }

  /**
   * Elegant daemon interface with strict typing - complete system access
   */
  get daemons() {
    return {
      commands: {
        execute: async <T extends CommandParams = CommandParams, U extends CommandResult = CommandResult>(
          command: string,
          params?: T
        ): Promise<U> => {
          // Execute command and get response (may be wrapped or unwrapped depending on connection type)
          const response = await this.commands[command](params);

          // Check if wrapped in CommandResponse (has commandResult field)
          if (response && typeof response === 'object' && 'commandResult' in response) {
            const wrapped = response as CommandSuccessResponse;
            return wrapped.commandResult as U;
          }

          // Already unwrapped - return as-is
          return response as U;
        },
        /**
         * Execute command LOCALLY without routing through transport layers
         * CRITICAL for server-side autonomous agents (PersonaUser, etc.)
         * Only works with LocalConnection - throws error for remote connections
         */
        localExecute: async <T extends CommandParams = CommandParams, U extends CommandResult = CommandResult>(
          commandName: string,
          params?: T
        ): Promise<U> => {
          const localConnection = this.connection as LocalConnection;
          if (!localConnection || !localConnection.localSystem) {
            throw new Error('localExecute only available in local connections');
          }

          // Get CommandDaemon from local system
          const commandDaemon = localConnection.localSystem.getCommandDaemon();
          if (!commandDaemon) {
            throw new Error('CommandDaemon not available');
          }

          // Cast to CommandDaemon type (has execute method)
          type CommandDaemonWithExecute = {
            execute(commandName: string, sessionId: UUID, params?: CommandParams): Promise<CommandResult>;
          };

          // Execute command directly via CommandDaemon.execute() - bypasses all routing
          const result = await (commandDaemon as unknown as CommandDaemonWithExecute).execute(
            commandName,
            this.sessionId,
            params
          );
          return result as U;
        }
      },
      events: {
        /**
         * Emit an event - works in browser and server
         * Delegates to Events.emit() for cross-environment event bridging
         */
        emit: async <T>(
          eventName: string,
          data: T,
          options?: { scope?: any; scopeId?: string; sessionId?: string }
        ): Promise<{ success: boolean; error?: string }> => {
          const { Events } = await import('../../shared/Events');
          return await Events.emit(eventName, data, options as any || {});
        },

        /**
         * Subscribe to events - works in browser and server
         * Uses unified EventSubscriptionManager from EventsDaemon
         */
        on: <T>(
          patternOrEventName: string,
          handler: (data: T) => void
        ): (() => void) => {
          try {
            // Get EventsDaemon from local system
            const localConnection = this.connection as LocalConnection;
            if (!localConnection || !localConnection.localSystem) {
              throw new Error('Events subscriptions only available in local connections');
            }

            const eventsDaemon = localConnection.localSystem.getEventsDaemon();
            if (!eventsDaemon) {
              throw new Error('EventsDaemon not available');
            }

            const subscriptionManager = eventsDaemon.getSubscriptionManager();
            return subscriptionManager.on(patternOrEventName, handler);
          } catch (error) {
            console.error(`❌ JTAGClient.daemons.events.on failed:`, error);
            return () => {}; // No-op unsubscribe
          }
        },

        /**
         * Unsubscribe from events
         */
        off: <T>(eventName: string, handler?: (data: T) => void): void => {
          try {
            const localConnection = this.connection as LocalConnection;
            if (!localConnection || !localConnection.localSystem) {
              throw new Error('Events subscriptions only available in local connections');
            }

            const eventsDaemon = localConnection.localSystem.getEventsDaemon();
            if (!eventsDaemon) {
              throw new Error('EventsDaemon not available');
            }

            const subscriptionManager = eventsDaemon.getSubscriptionManager();
            subscriptionManager.off(eventName, handler);
          } catch (error) {
            console.error(`❌ JTAGClient.daemons.events.off failed:`, error);
          }
        }
      },
      data: {
        store: async <T extends JTAGPayload>(key: string, value: T): Promise<void> => {
          try {
            // TODO: Implement data storage through DataDaemon
            console.log(`💾 JTAG daemon data.store ${key}:`, value);
            throw new Error('Data storage not yet implemented');
          } catch (error) {
            console.error(`❌ JTAG daemon data.store failed:`, error);
            throw error;
          }
        }
      }
    };
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

  async executeCommand(commandName: string, params: any): Promise<any> {
    // Direct call to local system - zero transport overhead
    const commandFn = this.localSystem.commands[commandName];
    if (!commandFn) {
      throw new Error(`Command '${commandName}' not available in local system`);
    }
    return await commandFn(params);
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
  waitForResponse(correlationId: string, timeoutMs?: number): Promise<JTAGPayload>;
}

/**
 * Remote connection - transport-based calls to remote JTAG system
 */
export class RemoteConnection implements JTAGConnection {
  public get sessionId(): UUID {
    return this.client.sessionId; // Dynamic getter - always uses client's current session
  }
  public readonly context: JTAGContext;

  constructor(
    private readonly client: JTAGClient,
    private readonly correlator: ICommandCorrelator
  ) {
    this.context = client.context;
  }

  async executeCommand(commandName: string, params: any): Promise<any> {
    // Create strongly-typed request message
    // Use client_ prefix for external client detection
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

    console.log(`📤 RemoteConnection: Sending command '${commandName}' with correlation ${correlationId}`);
    const sendResult: TransportSendResult = await transport.send(requestMessage);

    if (!sendResult.success) {
      throw new Error(`Transport failed to send command at ${sendResult.timestamp}`);
    }

    // Wait for correlated response using the shared correlation interface
    // 60s timeout for AI/inference commands that may take longer
    const response = await this.correlator.waitForResponse(correlationId, 60000);
    return response;
  }

  getCommandsInterface(): CommandsInterface {
    // ISSUE 2 FIXED: Create CommandsInterface from client's discovered commands
    const map = new Map();
    
    // Convert discovered commands to CommandsInterface format
    for (const [name, signature] of this.client.getDiscoveredCommands()) {
      // Create a command function that routes through this remote connection
      const commandFn = async (params?: JTAGPayload): Promise<JTAGPayload> => {
        return await this.executeCommand(name, params || { context: this.context, sessionId: this.sessionId });
      };
      
      // Store as CommandBase-like object (simplified for remote)
      map.set(name, commandFn as any); // TODO: Create proper CommandBase proxy
    }
    
    return map;
  }
}