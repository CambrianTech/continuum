/**
 * WebSocket Routing Types - Cross-Cutting Concern
 * 
 * Shared types for WebSocket routing service that handles all WebSocket-related
 * operations that were previously embedded in the SessionManagerDaemon.
 */

import { ContinuumContext } from '../../../types/shared/core/ContinuumTypes';

/**
 * WebSocket service logger interface
 */
export interface WebSocketServiceLogger {
  log(message: string, level?: 'info' | 'debug' | 'warn' | 'error'): void;
}

/**
 * WebSocket connection metadata from connection events
 */
export interface WebSocketConnectionMetadata {
  userAgent: string;
  url: string;
  headers: Record<string, string>;
}

/**
 * WebSocket routing service interface
 */
export interface IWebSocketRoutingService {
  /**
   * Initialize the WebSocket routing service
   */
  initialize(): Promise<void>;

  /**
   * Register with WebSocket daemon
   */
  registerWithWebSocketDaemon(webSocketDaemon: any): Promise<void>;

  /**
   * Send message to specific connection
   */
  sendToConnection(connectionId: string, message: any): Promise<void>;

  /**
   * Determine session type from connection metadata
   */
  determineSessionType(metadata: WebSocketConnectionMetadata): string;

  /**
   * Cleanup resources
   */
  cleanup(): Promise<void>;
}

/**
 * WebSocket routing service configuration
 */
export interface WebSocketRoutingConfig {
  context: ContinuumContext;
  logger: WebSocketServiceLogger;
  enableLegacyCompatibility?: boolean;
}

/**
 * WebSocket message handler registration data
 */
export interface WebSocketHandlerRegistration {
  messageType: string;
  handler: any;
  priority?: number;
  serviceName: string;
}

/**
 * WebSocket routing service status
 */
export interface WebSocketRoutingStatus {
  initialized: boolean;
  webSocketDaemonConnected: boolean;
  handlersRegistered: number;
  connectionsActive: number;
}