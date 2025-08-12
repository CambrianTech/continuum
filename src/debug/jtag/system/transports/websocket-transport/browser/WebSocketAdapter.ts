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
    switch (type) {
      case 'open':
        this.nativeSocket.addEventListener('open', (nativeEvent) => {
          const jtagEvent: JTAGWebSocketOpenEvent = { type: 'open' };
          listener(jtagEvent);
        });
        break;

      case 'message':
        this.nativeSocket.addEventListener('message', (nativeEvent) => {
          const jtagEvent: JTAGWebSocketMessageEvent = {
            type: 'message',
            data: nativeEvent.data
          };
          listener(jtagEvent);
        });
        break;

      case 'close':
        this.nativeSocket.addEventListener('close', (nativeEvent) => {
          const jtagEvent: JTAGWebSocketCloseEvent = {
            type: 'close',
            code: nativeEvent.code,
            reason: nativeEvent.reason
          };
          listener(jtagEvent);
        });
        break;

      case 'error':
        this.nativeSocket.addEventListener('error', (nativeEvent) => {
          const jtagEvent: JTAGWebSocketErrorEvent = {
            type: 'error',
            message: 'WebSocket error occurred'
          };
          listener(jtagEvent);
        });
        break;
    }
  }

  // Consistent removeEventListener implementation with JTAG-specific typing
  removeEventListener(type: 'open', listener: (event: JTAGWebSocketOpenEvent) => void): void;
  removeEventListener(type: 'message', listener: (event: JTAGWebSocketMessageEvent) => void): void;
  removeEventListener(type: 'close', listener: (event: JTAGWebSocketCloseEvent) => void): void;
  removeEventListener(type: 'error', listener: (event: JTAGWebSocketErrorEvent) => void): void;
  removeEventListener(type: string, listener: (event: any) => void): void {
    // Browser WebSocket removeEventListener requires the exact same function reference
    // This is a simplified implementation - in practice you'd need to track listeners
    console.warn(`removeEventListener not fully implemented for browser WebSocket adapter`);
  }
}