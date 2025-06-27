/**
 * Message Types - WebSocket message format interfaces
 */

export interface WebSocketMessage {
  readonly type: string;
  readonly data: any;
  readonly timestamp: string;
  readonly clientId?: string;
  readonly requestId?: string;
}

export interface CommandRequest {
  readonly command: string;
  readonly params: string;
  readonly encoding?: string;
  readonly requestId?: string;
  readonly timeout?: number;
}

export interface CommandResult {
  readonly success: boolean;
  readonly message?: string;
  readonly data?: any;
  readonly error?: string;
  readonly processor?: string;
  readonly duration?: number;
}

export interface EventMessage {
  readonly event: string;
  readonly payload: any;
  readonly source?: string;
  readonly priority?: 'low' | 'normal' | 'high' | 'critical';
}

export interface AuthRequest {
  readonly token?: string;
  readonly credentials?: Record<string, any>;
  readonly clientInfo?: ClientInfo;
}

export interface AuthResult {
  readonly success: boolean;
  readonly clientId?: string;
  readonly permissions?: string[];
  readonly error?: string;
}

export interface ClientInfo {
  readonly userAgent?: string;
  readonly origin?: string;
  readonly protocol?: string;
  readonly extensions?: string[];
}