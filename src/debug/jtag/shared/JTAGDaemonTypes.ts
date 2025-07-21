/**
 * JTAG Daemon Types - Symmetric Daemon Architecture
 * 
 * Following middle-out symmetric daemon patterns:
 * - Same DaemonMessage<T> protocol across all contexts
 * - Unified ProcessBasedDaemon interface
 * - Cross-context daemon communication
 */

import { 
  JTAGMessage, 
  JTAGContext, 
  JTAGLogPayload, 
  JTAGScreenshotPayload, 
  JTAGExecPayload,
  JTAGResponse 
} from './JTAGCoreTypes';

// ============================================
// SYMMETRIC DAEMON MESSAGE PROTOCOL
// ============================================

/**
 * Universal daemon message - works identically across all contexts
 * Server daemon ↔ Browser daemon ↔ Remote daemon
 */
export interface JTAGDaemonMessage<TPayload = any> extends JTAGMessage<TPayload> {
  fromDaemon: string;      // Source daemon identifier
  toDaemon?: string;       // Target daemon identifier (optional for broadcast)
  priority: JTAGMessagePriority;
  requiresResponse: boolean;
  correlationId?: string;
}

export type JTAGMessagePriority = 'low' | 'normal' | 'high' | 'critical';

// ============================================
// DAEMON RESPONSE PROTOCOL
// ============================================

export interface JTAGDaemonResponse<TData = any> extends JTAGResponse<TData> {
  processingTime: number;
  fromDaemon: string;
  correlationId?: string;
}

// ============================================
// PROCESS-BASED DAEMON INTERFACE
// ============================================

/**
 * Universal daemon interface - same shape across all contexts
 * - Server: Uses Node.js child processes or async queues
 * - Browser: Uses Web Workers or message queues
 * - Remote: Uses distributed message queues
 */
export interface ProcessBasedDaemon<TMessage extends JTAGDaemonMessage> {
  readonly name: string;
  readonly context: JTAGContext;
  readonly isRunning: boolean;
  
  // Lifecycle
  start(): Promise<void>;
  stop(): Promise<void>;
  restart(): Promise<void>;
  
  // Message processing (core daemon method)
  processMessage(message: TMessage): Promise<JTAGDaemonResponse>;
  
  // Communication
  send<TResponse = any>(
    message: TMessage, 
    timeout?: number
  ): Promise<JTAGDaemonResponse<TResponse>>;
  
  broadcast(message: TMessage): Promise<void>;
  
  // Health & Metrics
  isHealthy(): boolean;
  getMetrics(): JTAGDaemonMetrics;
}

export interface JTAGDaemonMetrics {
  messagesProcessed: number;
  averageProcessingTime: number;
  queueSize: number;
  errorCount: number;
  uptime: number;
  lastActivity: string;
  memoryUsage?: {
    heapUsed: number;
    heapTotal: number;
    external: number;
  };
}

// ============================================
// SPECIFIC DAEMON MESSAGE TYPES
// ============================================

// Logger Daemon Messages
export interface JTAGLoggerMessage extends JTAGDaemonMessage<JTAGLogPayload> {
  type: 'LOG';
}

// Screenshot Daemon Messages  
export interface JTAGScreenshotMessage extends JTAGDaemonMessage<JTAGScreenshotPayload> {
  type: 'SCREENSHOT';
}

// Exec Daemon Messages
export interface JTAGExecMessage extends JTAGDaemonMessage<JTAGExecPayload> {
  type: 'EXEC';
}

// Health Check Messages
export interface JTAGHealthMessage extends JTAGDaemonMessage<{ includeMetrics?: boolean }> {
  type: 'HEALTH_CHECK';
}

// ============================================
// DAEMON REGISTRY PROTOCOL
// ============================================

export interface JTAGDaemonRegistry {
  register(daemon: ProcessBasedDaemon<any>): void;
  unregister(daemonName: string): void;
  get(daemonName: string): ProcessBasedDaemon<any> | undefined;
  getByContext(context: JTAGContext): ProcessBasedDaemon<any>[];
  getAll(): ProcessBasedDaemon<any>[];
  route(message: JTAGDaemonMessage): Promise<JTAGDaemonResponse>;
}

// ============================================
// DAEMON FACTORY PROTOCOL
// ============================================

export interface JTAGDaemonFactory {
  createLogger(context: JTAGContext): ProcessBasedDaemon<JTAGLoggerMessage>;
  createScreenshot(context: JTAGContext): ProcessBasedDaemon<JTAGScreenshotMessage>;
  createExec(context: JTAGContext): ProcessBasedDaemon<JTAGExecMessage>;
}

// ============================================
// CROSS-CONTEXT COMMUNICATION PROTOCOL
// ============================================

/**
 * Enables symmetric communication between daemons across contexts
 * Server Logger Daemon ↔ Browser Logger Daemon
 */
export interface JTAGCrossContextBridge {
  // Server to Browser
  sendToBrowser<TResponse = any>(
    message: JTAGDaemonMessage,
    timeout?: number
  ): Promise<JTAGDaemonResponse<TResponse>>;
  
  // Browser to Server
  sendToServer<TResponse = any>(
    message: JTAGDaemonMessage,
    timeout?: number
  ): Promise<JTAGDaemonResponse<TResponse>>;
  
  // Remote to Any Context
  sendToRemote<TResponse = any>(
    message: JTAGDaemonMessage,
    targetNode: string,
    timeout?: number
  ): Promise<JTAGDaemonResponse<TResponse>>;
  
  // Broadcast to All Contexts
  broadcastToAll(message: JTAGDaemonMessage): Promise<void>;
}

// ============================================
// DAEMON LIFECYCLE EVENTS
// ============================================

export type JTAGDaemonEventType = 
  | 'started'
  | 'stopped' 
  | 'error'
  | 'message_received'
  | 'message_processed'
  | 'health_check';

export interface JTAGDaemonEvent {
  type: JTAGDaemonEventType;
  daemonName: string;
  context: JTAGContext;
  timestamp: string;
  data?: any;
  error?: Error;
}

export type JTAGDaemonEventListener = (event: JTAGDaemonEvent) => void;

export interface JTAGDaemonEventEmitter {
  on(eventType: JTAGDaemonEventType, listener: JTAGDaemonEventListener): void;
  off(eventType: JTAGDaemonEventType, listener: JTAGDaemonEventListener): void;
  emit(event: JTAGDaemonEvent): void;
}

// ============================================
// CONFIGURATION FOR DAEMON CONTEXTS
// ============================================

export interface JTAGServerDaemonConfig {
  context: 'SERVER';
  processPool?: {
    maxProcesses: number;
    idleTimeout: number;
    respawnOnExit: boolean;
  };
  asyncQueue?: {
    concurrency: number;
    maxRetries: number;
    retryDelay: number;
  };
}

export interface JTAGBrowserDaemonConfig {
  context: 'BROWSER';
  webWorkers?: {
    maxWorkers: number;
    workerTimeout: number;
    sharedArrayBuffer: boolean;
  };
  messageQueue?: {
    maxSize: number;
    batchSize: number;
    flushInterval: number;
  };
}

export interface JTAGRemoteDaemonConfig {
  context: 'REMOTE';
  distributedQueue?: {
    nodes: string[];
    replicationFactor: number;
    partitionStrategy: 'round-robin' | 'hash' | 'consistent-hash';
  };
  p2pMesh?: {
    discoveryNodes: string[];
    maxConnections: number;
    heartbeatInterval: number;
  };
}

export type JTAGDaemonConfig = 
  | JTAGServerDaemonConfig 
  | JTAGBrowserDaemonConfig 
  | JTAGRemoteDaemonConfig;