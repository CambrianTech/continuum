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

export class BrowserWebSocketAdapter implements JTAGUniversalWebSocket {
  private nativeSocket: WebSocket;
  private listenerMap: Map<any, any> = new Map(); // Track wrapper functions for cleanup

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
  addEventListener(type: string, listener: (event: any) => void): void {
    // Create wrapper function and store it for later removal
    const wrapper = (nativeEvent: any) => {
      let jtagEvent: any;
      switch (type) {
        case 'open':
          jtagEvent = { type: 'open' };
          break;
        case 'message':
          jtagEvent = { type: 'message', data: nativeEvent.data };
          break;
        case 'close':
          jtagEvent = { type: 'close', code: nativeEvent.code, reason: nativeEvent.reason };
          break;
        case 'error':
          jtagEvent = { type: 'error', message: 'WebSocket error occurred' };
          break;
      }
      listener(jtagEvent);
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
  removeEventListener(type: string, listener: (event: any) => void): void {
    // Get the stored wrapper function
    const wrapper = this.listenerMap.get(listener);
    if (wrapper) {
      // Remove using the exact wrapper function reference
      this.nativeSocket.removeEventListener(type, wrapper);
      this.listenerMap.delete(listener);
    }
  }
}