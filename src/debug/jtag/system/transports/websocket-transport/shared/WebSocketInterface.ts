/**
 * Universal WebSocket Interface - Consistent typing across environments
 * 
 * Provides type-safe abstraction over browser WebSocket and Node.js ws library.
 */

// Universal WebSocket interface that both environments must implement
export interface UniversalWebSocket {
  // Core WebSocket methods
  send(data: string): void;
  close(code?: number, reason?: string): void;
  
  // WebSocket state
  readonly readyState: number;
  readonly url: string;
  
  // Consistent event handling - addEventListener pattern
  addEventListener(type: 'open', listener: (event: WebSocketOpenEvent) => void): void;
  addEventListener(type: 'message', listener: (event: WebSocketMessageEvent) => void): void;
  addEventListener(type: 'close', listener: (event: WebSocketCloseEvent) => void): void;
  addEventListener(type: 'error', listener: (event: WebSocketErrorEvent) => void): void;
  
  removeEventListener(type: 'open', listener: (event: WebSocketOpenEvent) => void): void;
  removeEventListener(type: 'message', listener: (event: WebSocketMessageEvent) => void): void;
  removeEventListener(type: 'close', listener: (event: WebSocketCloseEvent) => void): void;
  removeEventListener(type: 'error', listener: (event: WebSocketErrorEvent) => void): void;
}

// Universal event interfaces
export interface WebSocketOpenEvent {
  type: 'open';
}

export interface WebSocketMessageEvent {
  type: 'message';
  data: string | Buffer | ArrayBuffer | any; // Allow for different WebSocket data types
}

export interface WebSocketCloseEvent {
  type: 'close';
  code: number;
  reason: string;
}

export interface WebSocketErrorEvent {
  type: 'error';
  error?: Error;
  message?: string;
}

// WebSocket ready states (standard)
export const WebSocketReadyState = {
  CONNECTING: 0,
  OPEN: 1,
  CLOSING: 2,
  CLOSED: 3
} as const;

export type WebSocketReadyStateValue = typeof WebSocketReadyState[keyof typeof WebSocketReadyState];