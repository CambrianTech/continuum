/**
 * WebSocket Communication Types
 * Types for WebSocket messages and protocols
 */

export interface WebSocketMessage {
  type: string;
  data?: Record<string, unknown>;
  timestamp: string;
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