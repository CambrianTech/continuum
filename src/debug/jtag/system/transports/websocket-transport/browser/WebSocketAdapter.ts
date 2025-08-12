/**
 * Browser WebSocket Adapter - Makes browser WebSocket conform to UniversalWebSocket interface
 * 
 * Adapts native browser WebSocket to use consistent addEventListener pattern.
 */

import type { 
  UniversalWebSocket, 
  WebSocketOpenEvent, 
  WebSocketMessageEvent, 
  WebSocketCloseEvent, 
  WebSocketErrorEvent 
} from '../shared/WebSocketInterface';

export class BrowserWebSocketAdapter implements UniversalWebSocket {
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

  get readyState(): number {
    return this.nativeSocket.readyState;
  }

  get url(): string {
    return this.nativeSocket.url;
  }

  // Consistent addEventListener implementation
  addEventListener(type: 'open', listener: (event: WebSocketOpenEvent) => void): void;
  addEventListener(type: 'message', listener: (event: WebSocketMessageEvent) => void): void;
  addEventListener(type: 'close', listener: (event: WebSocketCloseEvent) => void): void;
  addEventListener(type: 'error', listener: (event: WebSocketErrorEvent) => void): void;
  addEventListener(type: string, listener: (event: any) => void): void {
    switch (type) {
      case 'open':
        this.nativeSocket.addEventListener('open', (nativeEvent) => {
          const universalEvent: WebSocketOpenEvent = { type: 'open' };
          listener(universalEvent);
        });
        break;

      case 'message':
        this.nativeSocket.addEventListener('message', (nativeEvent) => {
          const universalEvent: WebSocketMessageEvent = {
            type: 'message',
            data: nativeEvent.data
          };
          listener(universalEvent);
        });
        break;

      case 'close':
        this.nativeSocket.addEventListener('close', (nativeEvent) => {
          const universalEvent: WebSocketCloseEvent = {
            type: 'close',
            code: nativeEvent.code,
            reason: nativeEvent.reason
          };
          listener(universalEvent);
        });
        break;

      case 'error':
        this.nativeSocket.addEventListener('error', (nativeEvent) => {
          const universalEvent: WebSocketErrorEvent = {
            type: 'error',
            message: 'WebSocket error occurred'
          };
          listener(universalEvent);
        });
        break;
    }
  }

  // Consistent removeEventListener implementation
  removeEventListener(type: 'open', listener: (event: WebSocketOpenEvent) => void): void;
  removeEventListener(type: 'message', listener: (event: WebSocketMessageEvent) => void): void;
  removeEventListener(type: 'close', listener: (event: WebSocketCloseEvent) => void): void;
  removeEventListener(type: 'error', listener: (event: WebSocketErrorEvent) => void): void;
  removeEventListener(type: string, listener: (event: any) => void): void {
    // Browser WebSocket removeEventListener requires the exact same function reference
    // This is a simplified implementation - in practice you'd need to track listeners
    console.warn(`removeEventListener not fully implemented for browser WebSocket adapter`);
  }
}