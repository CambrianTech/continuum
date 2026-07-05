/**
 * Browser WebSocket Adapter - Makes browser WebSocket conform to JTAGUniversalWebSocket interface
 *
 * Adapts native browser WebSocket to use consistent addEventListener pattern with JTAG-specific typing.
 */

import type {
  JTAGUniversalWebSocket,
  JTAGWebSocketOpenEvent,
  JTAGWebSocketMessageEvent,
  JTAGWebSocketCloseEvent,
  JTAGWebSocketErrorEvent,
  JTAGWebSocketReadyState
} from '../shared/WebSocketInterface';

/** Union of all JTAG WebSocket event types */
type JTAGWebSocketEvent = JTAGWebSocketOpenEvent | JTAGWebSocketMessageEvent | JTAGWebSocketCloseEvent | JTAGWebSocketErrorEvent;

/** Listener function that accepts a specific JTAG event (union of all specific listener signatures) */
type JTAGEventListener =
  | ((event: JTAGWebSocketOpenEvent) => void)
  | ((event: JTAGWebSocketMessageEvent) => void)
  | ((event: JTAGWebSocketCloseEvent) => void)
  | ((event: JTAGWebSocketErrorEvent) => void);

/** Native browser event listener wrapper */
type NativeEventWrapper = (event: Event) => void;

export class BrowserWebSocketAdapter implements JTAGUniversalWebSocket {
  private nativeSocket: WebSocket;
  // Map from original listener reference to native wrapper — keyed by identity for cleanup
  private listenerMap: Map<JTAGEventListener, NativeEventWrapper> = new Map();

  constructor(url: string) {
    this.nativeSocket = new WebSocket(url);
  }

  // Delegate core methods to native WebSocket
  send(data: string): void {
    this.nativeSocket.send(data);
  }

  close(code?: number, reason?: string): void {
    this.nativeSocket.close(code, reason);
  }

  get readyState(): JTAGWebSocketReadyState {
    return this.nativeSocket.readyState as JTAGWebSocketReadyState;
  }

  get url(): string {
    return this.nativeSocket.url;
  }

  // Consistent addEventListener implementation with JTAG-specific typing
  addEventListener(type: 'open', listener: (event: JTAGWebSocketOpenEvent) => void): void;
  addEventListener(type: 'message', listener: (event: JTAGWebSocketMessageEvent) => void): void;
  addEventListener(type: 'close', listener: (event: JTAGWebSocketCloseEvent) => void): void;
  addEventListener(type: 'error', listener: (event: JTAGWebSocketErrorEvent) => void): void;
  addEventListener(type: string, listener: JTAGEventListener): void {
    // Create wrapper function and store it for later removal
    const wrapper: NativeEventWrapper = (nativeEvent: Event) => {
      switch (type) {
        case 'open':
          (listener as (event: JTAGWebSocketOpenEvent) => void)({ type: 'open' });
          break;
        case 'message':
          (listener as (event: JTAGWebSocketMessageEvent) => void)({
            type: 'message',
            data: (nativeEvent as MessageEvent).data
          });
          break;
        case 'close': {
          const closeEvt = nativeEvent as CloseEvent;
          (listener as (event: JTAGWebSocketCloseEvent) => void)({
            type: 'close',
            code: closeEvt.code,
            reason: closeEvt.reason
          });
          break;
        }
        case 'error':
          (listener as (event: JTAGWebSocketErrorEvent) => void)({
            type: 'error',
            message: 'WebSocket error occurred'
          });
          break;
      }
    };

    // Store wrapper for removal
    this.listenerMap.set(listener, wrapper);

    // Add wrapper to native socket
    this.nativeSocket.addEventListener(type, wrapper);
  }

  // Consistent removeEventListener implementation with JTAG-specific typing
  removeEventListener(type: 'open', listener: (event: JTAGWebSocketOpenEvent) => void): void;
  removeEventListener(type: 'message', listener: (event: JTAGWebSocketMessageEvent) => void): void;
  removeEventListener(type: 'close', listener: (event: JTAGWebSocketCloseEvent) => void): void;
  removeEventListener(type: 'error', listener: (event: JTAGWebSocketErrorEvent) => void): void;
  removeEventListener(type: string, listener: JTAGEventListener): void {
    // Get the stored wrapper function
    const wrapper = this.listenerMap.get(listener);
    if (wrapper) {
      // Remove using the exact wrapper function reference
      this.nativeSocket.removeEventListener(type, wrapper);
      this.listenerMap.delete(listener);
    }
  }
}
