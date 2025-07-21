/**
 * JTAG Core Types - Universal Module Structure
 * 
 * Following middle-out architecture patterns:
 * - Shared types that work across ALL contexts
 * - Clear separation between protocols and implementations
 * - Symmetric daemon messaging patterns
 */

// ============================================
// UNIVERSAL MESSAGE PROTOCOL
// ============================================

export interface JTAGMessage<TPayload = any> {
  id: string;
  type: JTAGMessageType;
  context: JTAGContext;
  timestamp: number;
  payload: TPayload;
}

export type JTAGMessageType = 
  | 'LOG'
  | 'SCREENSHOT' 
  | 'EXEC'
  | 'HEALTH_CHECK'
  | 'CONNECT';

export type JTAGContext = 
  | 'BROWSER'
  | 'SERVER' 
  | 'REMOTE'
  | 'TEST';

// ============================================
// LOGGING PROTOCOL - UNIVERSAL
// ============================================

export interface JTAGLogMessage extends JTAGMessage<JTAGLogPayload> {
  type: 'LOG';
}

export interface JTAGLogPayload {
  level: JTAGLogLevel;
  component: string;
  message: string;
  data?: any;
  correlationId?: string;
  stackTrace?: string;
}

export type JTAGLogLevel = 
  | 'log' 
  | 'info'
  | 'warn' 
  | 'error' 
  | 'critical' 
  | 'trace' 
  | 'probe'
  | 'debug';

// ============================================
// FILE PROTOCOL - CONTEXT-SPECIFIC
// ============================================

/**
 * Universal file naming protocol: {context}.{level}.{extension}
 * Examples: server.error.json, browser.log.txt, remote.critical.json
 */
export interface JTAGLogFile {
  context: Lowercase<JTAGContext>;
  level: JTAGLogLevel;
  extension: 'json' | 'txt';
  path: string;
}

export interface JTAGLogEntry {
  timestamp: string;
  context: JTAGContext;
  component: string; 
  message: string;
  level: JTAGLogLevel;
  data?: any;
  correlationId?: string;
}

// ============================================
// SCREENSHOT PROTOCOL - UNIVERSAL  
// ============================================

export interface JTAGScreenshotMessage extends JTAGMessage<JTAGScreenshotPayload> {
  type: 'SCREENSHOT';
}

export interface JTAGScreenshotPayload {
  filename: string;
  selector?: string;
  options?: JTAGScreenshotOptions;
  dataUrl?: string;
  filepath?: string;
  urgent?: boolean;
}

export interface JTAGScreenshotOptions {
  width?: number;
  height?: number;
  format?: 'png' | 'jpg' | 'webp';
  quality?: number;
  fullPage?: boolean;
  clip?: {
    x: number;
    y: number; 
    width: number;
    height: number;
  };
}

export interface JTAGScreenshotResult {
  success: boolean;
  filename?: string;
  filepath?: string;
  dataUrl?: string;
  error?: string;
  metadata?: {
    timestamp: string;
    context: JTAGContext;
    dimensions?: { width: number; height: number };
    fileSize?: number;
  };
}

// ============================================
// CODE EXECUTION PROTOCOL - UNIVERSAL
// ============================================

export interface JTAGExecMessage extends JTAGMessage<JTAGExecPayload> {
  type: 'EXEC';
}

export interface JTAGExecPayload {
  code: string;
  options?: JTAGExecOptions;
  timeout?: number;
  environment?: Record<string, any>;
}

export interface JTAGExecOptions {
  timeout?: number;
  returnType?: 'json' | 'string' | 'raw';
  captureStdout?: boolean;
  captureStderr?: boolean;
  context?: 'global' | 'isolated';
}

export interface JTAGExecResult {
  success: boolean;
  result?: any;
  error?: string;
  stdout?: string;
  stderr?: string;
  executionTime?: number;
  context: JTAGContext;
  timestamp: string;
}

// ============================================
// HEALTH CHECK PROTOCOL - UNIVERSAL
// ============================================

export interface JTAGHealthMessage extends JTAGMessage<JTAGHealthPayload> {
  type: 'HEALTH_CHECK';
}

export interface JTAGHealthPayload {
  includeMetrics?: boolean;
  includeTransports?: boolean;
  checkConnections?: boolean;
}

export interface JTAGHealthResult {
  healthy: boolean;
  context: JTAGContext;
  uptime: number;
  metrics?: JTAGMetrics;
  transports?: JTAGTransportStatus[];
  connections?: JTAGConnectionStatus[];
  timestamp: string;
}

export interface JTAGMetrics {
  messagesProcessed: number;
  logsWritten: number;
  screenshotsTaken: number;
  execsRun: number;
  errors: number;
  averageResponseTime: number;
}

export interface JTAGTransportStatus {
  name: string;
  active: boolean;
  messageCount: number;
  lastActivity: string;
  error?: string;
}

export interface JTAGConnectionStatus {
  type: 'websocket' | 'http' | 'file' | 'memory';
  state: 'connected' | 'disconnected' | 'error';
  endpoint?: string;
  latency?: number;
  lastActivity: string;
}

// ============================================
// UUID PROTOCOL - UNIVERSAL
// ============================================

export interface JTAGUUIDInfo {
  uuid: string;
  sessionId: string;
  context: JTAGContext;
  timestamp: string;
  uptime?: number;
}

// ============================================
// RESPONSE PROTOCOL - UNIVERSAL
// ============================================

export interface JTAGResponse<TData = any> {
  success: boolean;
  data?: TData;
  error?: string;
  context: JTAGContext;
  timestamp: string;
  requestId?: string;
}

// ============================================
// CONFIG PROTOCOL - CONTEXT-SPECIFIC
// ============================================

export interface JTAGBaseConfig {
  context: JTAGContext;
  enableRemoteLogging: boolean;
  enableConsoleOutput: boolean;
  maxBufferSize: number;
}

export interface JTAGServerConfig extends JTAGBaseConfig {
  context: 'SERVER';
  jtagPort: number;
  logDirectory: string;
  screenshotDirectory: string;
  rootPath: string;
}

export interface JTAGBrowserConfig extends JTAGBaseConfig {
  context: 'BROWSER';
  serverEndpoint?: string;
  reconnectAttempts?: number;
  reconnectDelay?: number;
}

export interface JTAGRemoteConfig extends JTAGBaseConfig {
  context: 'REMOTE';
  meshNodes?: string[];
  distributedLogging?: boolean;
}

export type JTAGConfig = JTAGServerConfig | JTAGBrowserConfig | JTAGRemoteConfig;

// ============================================
// TRANSPORT PROTOCOL - UNIVERSAL
// ============================================

export interface JTAGTransportBackend {
  readonly name: string;
  process(message: JTAGMessage): Promise<JTAGResponse>;
  isHealthy(): boolean;
  getMetrics(): JTAGTransportMetrics;
}

export interface JTAGTransportMetrics {
  messagesProcessed: number;
  lastActivity: string;
  avgResponseTime: number;
  errorCount: number;
}

// ============================================
// ERROR TYPES - UNIVERSAL  
// ============================================

export class JTAGError extends Error {
  constructor(
    message: string,
    public context: JTAGContext,
    public component: string,
    public code?: string
  ) {
    super(message);
    this.name = 'JTAGError';
  }
}

export class JTAGTransportError extends JTAGError {
  constructor(message: string, context: JTAGContext, public transportName: string) {
    super(message, context, 'TRANSPORT');
    this.name = 'JTAGTransportError';
  }
}

export class JTAGConfigError extends JTAGError {
  constructor(message: string, context: JTAGContext) {
    super(message, context, 'CONFIG');
    this.name = 'JTAGConfigError';
  }
}