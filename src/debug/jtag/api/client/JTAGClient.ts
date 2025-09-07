/**
 * JTAG Client API - Public Interface
 * 
 * Consumer-first API that wraps the internal JTAGClient with clean, typed interfaces.
 * External consumers import and use this client for all JTAG operations.
 */

import type { 
  FileCommandParams, 
  FileCommandResult,
  ChatCommandParams,
  ChatCommandResult, 
  ScreenshotCommandParams,
  ScreenshotCommandResult,
  SystemCommandParams,
  SystemCommandResult
} from '../commands';

// Client connection and lifecycle
export interface ClientConnectionOptions {
  /** Target environment for connection */
  environment?: 'server' | 'browser';
  /** Transport protocol to use */
  transport?: 'websocket' | 'http';
  /** Server URL for remote connections */
  serverUrl?: string;
  /** Connection timeout in milliseconds */
  timeout?: number;
  /** Enable automatic reconnection */
  autoReconnect?: boolean;
  /** Maximum number of connection retries */
  maxRetries?: number;
}

export interface ClientConnectionResult {
  success: boolean;
  client?: IJTAGClient;
  environment: 'server' | 'browser';
  connectionType: 'local' | 'remote';
  error?: string;
}

// Main client interface - typed command methods
export interface IJTAGClient {
  /** Client connection information */
  readonly connected: boolean;
  readonly environment: 'server' | 'browser';  
  readonly connectionType: 'local' | 'remote';

  // File Operations
  fileLoad(params: FileCommandParams): Promise<FileCommandResult>;
  fileSave(params: FileCommandParams): Promise<FileCommandResult>;
  fileAppend(params: FileCommandParams): Promise<FileCommandResult>;
  fileExists(params: FileCommandParams): Promise<FileCommandResult>;
  fileDelete(params: FileCommandParams): Promise<FileCommandResult>;
  directoryList(params: FileCommandParams): Promise<FileCommandResult>;

  // Chat Operations  
  chatSendMessage(params: ChatCommandParams): Promise<ChatCommandResult>;
  chatCreateRoom(params: ChatCommandParams): Promise<ChatCommandResult>;
  chatJoinRoom(params: ChatCommandParams): Promise<ChatCommandResult>;
  chatLeaveRoom(params: ChatCommandParams): Promise<ChatCommandResult>;
  chatGetHistory(params: ChatCommandParams): Promise<ChatCommandResult>;
  chatListRooms(params: ChatCommandParams): Promise<ChatCommandResult>;
  chatUpdateStatus(params: ChatCommandParams): Promise<ChatCommandResult>;
  chatSubscribeEvents(params: ChatCommandParams): Promise<ChatCommandResult>;

  // Screenshot Operations
  screenshot(params: ScreenshotCommandParams): Promise<ScreenshotCommandResult>;
  screenshotBatch(params: ScreenshotCommandParams): Promise<ScreenshotCommandResult>;
  screenshotGetElementInfo(params: ScreenshotCommandParams): Promise<ScreenshotCommandResult>;
  screenshotGetPageInfo(params: ScreenshotCommandParams): Promise<ScreenshotCommandResult>;
  screenshotCompare(params: ScreenshotCommandParams): Promise<ScreenshotCommandResult>;

  // System Operations
  systemHealth(params?: SystemCommandParams): Promise<SystemCommandResult>;
  systemGetConfig(params?: SystemCommandParams): Promise<SystemCommandResult>;
  systemSetConfig(params: SystemCommandParams): Promise<SystemCommandResult>;
  systemListProcesses(params?: SystemCommandParams): Promise<SystemCommandResult>;
  systemStartService(params: SystemCommandParams): Promise<SystemCommandResult>;
  systemStopService(params: SystemCommandParams): Promise<SystemCommandResult>;
  systemExecuteCommand(params: SystemCommandParams): Promise<SystemCommandResult>;
  systemGetLogs(params?: SystemCommandParams): Promise<SystemCommandResult>;

  // Generic command execution (for advanced usage)
  executeCommand<TParams, TResult>(commandName: string, params: TParams): Promise<TResult>;

  // Connection management
  disconnect(): Promise<void>;
  reconnect(): Promise<ClientConnectionResult>;
}

// Client factory functions
export interface ClientFactory {
  /** Connect to local JTAG system */
  connectLocal(options?: ClientConnectionOptions): Promise<ClientConnectionResult>;
  
  /** Connect to remote JTAG system */  
  connectRemote(serverUrl: string, options?: ClientConnectionOptions): Promise<ClientConnectionResult>;
  
  /** Auto-detect and connect (local first, then remote) */
  connect(options?: ClientConnectionOptions): Promise<ClientConnectionResult>;
}

// Error handling
export class JTAGClientError extends Error {
  constructor(
    message: string,
    public readonly code: string,
    public readonly details?: any
  ) {
    super(message);
    this.name = 'JTAGClientError';
  }
}

export const JTAGErrorCodes = {
  CONNECTION_FAILED: 'CONNECTION_FAILED',
  COMMAND_FAILED: 'COMMAND_FAILED',
  TIMEOUT: 'TIMEOUT',
  INVALID_PARAMS: 'INVALID_PARAMS',
  PERMISSION_DENIED: 'PERMISSION_DENIED',
  SERVICE_UNAVAILABLE: 'SERVICE_UNAVAILABLE'
} as const;

// Events for real-time updates
export interface ClientEvent {
  type: 'connected' | 'disconnected' | 'reconnecting' | 'error' | 'command_complete';
  data?: any;
  timestamp: string;
}

export interface IClientEventListener {
  (event: ClientEvent): void;
}

export interface IClientEventEmitter {
  on(eventType: ClientEvent['type'], listener: IClientEventListener): void;
  off(eventType: ClientEvent['type'], listener: IClientEventListener): void;
  emit(event: ClientEvent): void;
}

// Command execution context and metadata
export interface CommandContext {
  sessionId?: string;
  userId?: string;
  requestId?: string;
  timeout?: number;
  metadata?: Record<string, any>;
}

export interface CommandResult<T> {
  success: boolean;
  data?: T;
  error?: {
    code: string;
    message: string;
    details?: any;
  };
  metadata?: {
    executionTime?: number;
    requestId?: string;
    timestamp: string;
  };
}

// Note: All types are already exported above, no need for duplicate exports