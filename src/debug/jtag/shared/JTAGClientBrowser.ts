/**
 * JTAGClientBrowser - Strongly-typed thin client with local/remote connection abstraction
 * 
 * Provides identical command interface whether connecting locally (direct) or remotely (transport).
 * No cross-cutting concerns - clean separation between connection strategies and command execution.
 */

import type { JTAGContext, JTAGMessage, JTAGPayload } from './JTAGTypes';
import { JTAGMessageFactory } from './JTAGTypes';
import { generateUUID, type UUID } from './CrossPlatformUUID';
import { JTAG_ENVIRONMENTS } from './JTAGTypes';
import type { JTAGSystemBrowser } from '../browser/JTAGSystemBrowser';
import { JTAGClient, type JTAGClientConnectOptions } from './JTAGClient';

// Import all command types for strong typing
import type { ScreenshotParams, ScreenshotResult } from '@commandsScreenshot/shared/ScreenshotTypes';
import type { NavigateParams, NavigateResult } from '@commandsNavigate/shared/NavigateTypes';
import type { ClickParams, ClickResult } from '@commandsClick/shared/ClickTypes';
import type { TypeParams, TypeResult } from '@commandsType/shared/TypeTypes';

/**
 * Strongly-typed commands interface for JTAGClientBrowser
 */
export interface JTAGClientBrowserCommands {
  screenshot(params?: Partial<ScreenshotParams>): Promise<ScreenshotResult>;
  navigate(params: NavigateParams): Promise<NavigateResult>;
  click(params: ClickParams): Promise<ClickResult>;
  type(params: TypeParams): Promise<TypeResult>;
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
 * Local connection - direct calls to JTAGSystemBrowser (no transport)
 */
export class LocalConnection implements JTAGConnection {
  public readonly sessionId: UUID;
  public readonly context: JTAGContext;

  constructor(private readonly localSystem: JTAGSystemBrowser) {
    this.sessionId = localSystem.sessionId;
    this.context = localSystem.context;
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
      this.context.environment, // origin: browser
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

/**
 * Connection configuration for remote connections
 */
export interface RemoteConnectionConfig {
  readonly serverUrl: string;
  readonly sessionId?: UUID;
  readonly maxRetries?: number;
  readonly retryDelay?: number;
  readonly transportType?: 'websocket' | 'http';
}

/**
 * JTAGClientBrowser - Thin client with connection abstraction
 */
export class JTAGClientBrowser {
  private constructor(private readonly connection: JTAGConnection) {}

  /**
   * Connect to local JTAGSystemBrowser instance (direct calls)
   */
  static async connectLocal(): Promise<JTAGClientBrowser> {
    // Dynamic import to avoid circular dependency
    const { JTAGSystemBrowser } = await import('../browser/JTAGSystemBrowser');
    
    // Get direct reference to local system
    const localSystem = await JTAGSystemBrowser.connect();
    const connection = new LocalConnection(localSystem);
    
    return new JTAGClientBrowser(connection);
  }

  /**
   * Connect to remote JTAG system via transport
   */
  static async connectRemote(config: RemoteConnectionConfig): Promise<JTAGClientBrowser> {
    const clientOptions: JTAGClientConnectOptions = {
      targetEnvironment: 'server', // Remote system is typically server
      transportType: config.transportType ?? 'websocket',
      serverUrl: config.serverUrl,
      sessionId: config.sessionId,
      maxRetries: config.maxRetries ?? 30,
      retryDelay: config.retryDelay ?? 1000
    };

    const client = await JTAGClient.connect(clientOptions);
    const connection = new RemoteConnection(client);
    
    return new JTAGClientBrowser(connection);
  }

  /**
   * Strongly-typed commands interface
   */
  get commands(): JTAGClientBrowserCommands {
    return {
      screenshot: async (params?: Partial<ScreenshotParams>): Promise<ScreenshotResult> => {
        const fullParams = {
          ...params,
          context: params?.context ?? this.connection.context,
          sessionId: params?.sessionId ?? this.connection.sessionId
        } as ScreenshotParams;
        
        return await this.connection.executeCommand<ScreenshotParams, ScreenshotResult>(
          'screenshot', 
          fullParams
        );
      },

      navigate: async (params: NavigateParams): Promise<NavigateResult> => {
        const fullParams = {
          ...params,
          context: params.context ?? this.connection.context,
          sessionId: params.sessionId ?? this.connection.sessionId
        } as NavigateParams;
        
        return await this.connection.executeCommand<NavigateParams, NavigateResult>(
          'navigate', 
          fullParams
        );
      },

      click: async (params: ClickParams): Promise<ClickResult> => {
        const fullParams = {
          ...params,
          context: params.context ?? this.connection.context,
          sessionId: params.sessionId ?? this.connection.sessionId
        } as ClickParams;
        
        return await this.connection.executeCommand<ClickParams, ClickResult>(
          'click', 
          fullParams
        );
      },

      type: async (params: TypeParams): Promise<TypeResult> => {
        const fullParams = {
          ...params,
          context: params.context ?? this.connection.context,
          sessionId: params.sessionId ?? this.connection.sessionId
        } as TypeParams;
        
        return await this.connection.executeCommand<TypeParams, TypeResult>(
          'type', 
          fullParams
        );
      }
    };
  }

  /**
   * Connection metadata
   */
  get sessionId(): UUID {
    return this.connection.sessionId;
  }

  get context(): JTAGContext {
    return this.connection.context;
  }

  get isLocal(): boolean {
    return this.connection instanceof LocalConnection;
  }

  get isRemote(): boolean {
    return this.connection instanceof RemoteConnection;
  }
}