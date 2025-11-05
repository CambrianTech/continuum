/**
 * Connection Manager Message Types - Typed messages for WebSocket connection lifecycle
 * Uses DaemonMessage<T> generic pattern for type safety
 */

import { DaemonMessage } from '../../base/DaemonProtocol';

export interface ConnectionMessage {
  type: 'register' | 'unregister' | 'map_session' | 'unmap_session' | 'list' | 'send_to_connection' | 'check_connection';
  payload: 
    | ConnectionRegisterRequest 
    | ConnectionUnregisterRequest 
    | ConnectionMapSessionRequest 
    | ConnectionUnmapSessionRequest 
    | ConnectionListRequest
    | SendToConnectionRequest
    | CheckConnectionRequest;
}

export interface ConnectionRegisterRequest {
  connectionId: string;
  metadata?: ConnectionMetadata;
}

export interface ConnectionUnregisterRequest {
  connectionId: string;
  reason?: string;
}

export interface ConnectionMapSessionRequest {
  connectionId: string;
  sessionId: string;
}

export interface ConnectionUnmapSessionRequest {
  connectionId: string;
}

export interface ConnectionListRequest {
  sessionId?: string;
  activeOnly?: boolean;
}

export interface SendToConnectionRequest {
  connectionId: string;
  data: any;
}

export interface CheckConnectionRequest {
  connectionId: string;
}

export interface ConnectionMetadata {
  remoteAddress?: string;
  userAgent?: string;
  timestamp: Date;
  sessionId?: string;
}

export interface ConnectionInfo {
  id: string;
  sessionId?: string | undefined;
  metadata: ConnectionMetadata;
  isActive: boolean;
  lastActivity: Date;
}

export interface ConnectionEvent {
  type: 'connected' | 'disconnected' | 'session_mapped' | 'session_unmapped';
  connectionId: string;
  sessionId?: string | undefined;
  timestamp: Date;
  metadata?: Record<string, any>;
}

// Typed daemon messages for connection manager
export type ConnectionDaemonMessage = DaemonMessage<ConnectionMessage>;

// Helper function to create connection daemon messages
export function createConnectionMessage(
  type: ConnectionMessage['type'], 
  payload: ConnectionMessage['payload']
): ConnectionDaemonMessage {
  return {
    id: crypto.randomUUID(),
    from: 'connection-manager-client',
    to: 'connection-manager',
    type: 'daemon',
    timestamp: new Date(),
    data: {
      type,
      payload
    }
  };
}