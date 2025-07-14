/**
 * WebSocket Communication Types
 * Types for WebSocket messages and protocols
 */

export interface WebSocketMessage {
  type: string;
  data?: Record<string, unknown>;
  timestamp?: string; // Optional - sendMessage will add it
  clientId?: string | null;
  sessionId?: string | null;
}

export interface ClientInitData extends Record<string, unknown> {
  userAgent: string;
  url: string;
  timestamp: string;
  version: string;
  mode: string;
}

export interface CommandExecuteData extends Record<string, unknown> {
  command: string;
  params: string;
  requestId: string;
  sessionId: string | null;
}

export interface RemoteExecutionRequest extends Record<string, unknown> {
  command: string;
  params: Record<string, unknown>;
  requestId: string;
  sessionId: string | null;
  timeout?: number;
}

export interface RemoteExecutionResponse extends Record<string, unknown> {
  success: boolean;
  data?: unknown;
  error?: string;
  requestId: string;
  clientMetadata?: {
    userAgent: string;
    timestamp: number;
    executionTime: number;
  };
}