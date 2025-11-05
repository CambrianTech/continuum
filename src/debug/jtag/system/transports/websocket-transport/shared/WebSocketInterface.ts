/**
 * JTAG WebSocket Interface - Specific typing for our JTAG protocol
 * 
 * We control both ends of communication, so we can be very specific
 * about what we send/receive instead of using generic WebSocket types.
 */

import type { 
  JTAGWebSocketOpenEvent,
  JTAGWebSocketMessageEvent,
  JTAGWebSocketCloseEvent,
  JTAGWebSocketErrorEvent,
  JTAGWebSocketReadyState
} from './JTAGWebSocketTypes';

// JTAG-specific WebSocket interface that both environments must implement
export interface JTAGUniversalWebSocket {
  // Core WebSocket methods - only what we need for JTAG
  send(data: string): void;
  close(code?: number, reason?: string): void;
  
  // WebSocket state using JTAG constants
  readonly readyState: JTAGWebSocketReadyState;
  readonly url: string;
  
  // JTAG-specific event handling with proper typing
  addEventListener(type: 'open', listener: (event: JTAGWebSocketOpenEvent) => void): void;
  addEventListener(type: 'message', listener: (event: JTAGWebSocketMessageEvent) => void): void;
  addEventListener(type: 'close', listener: (event: JTAGWebSocketCloseEvent) => void): void;
  addEventListener(type: 'error', listener: (event: JTAGWebSocketErrorEvent) => void): void;
  
  removeEventListener(type: 'open', listener: (event: JTAGWebSocketOpenEvent) => void): void;
  removeEventListener(type: 'message', listener: (event: JTAGWebSocketMessageEvent) => void): void;
  removeEventListener(type: 'close', listener: (event: JTAGWebSocketCloseEvent) => void): void;
  removeEventListener(type: 'error', listener: (event: JTAGWebSocketErrorEvent) => void): void;
}

// Re-export JTAG types for convenience
export type { 
  JTAGWebSocketOpenEvent,
  JTAGWebSocketMessageEvent,
  JTAGWebSocketCloseEvent,
  JTAGWebSocketErrorEvent,
  JTAGWebSocketEventType,
  JTAGWebSocketReadyState
} from './JTAGWebSocketTypes';

// Re-export constants
export { JTAG_WEBSOCKET_READY_STATE, JTAG_WEBSOCKET_EVENTS } from './JTAGWebSocketTypes';