/**
 * Server WebSocket Adapter - Makes Node.js ws WebSocket conform to UniversalWebSocket interface
 * 
 * Adapts Node.js ws WebSocket to use consistent addEventListener pattern.
 */

import { WebSocket as WSWebSocket } from 'ws';
import type { 
  UniversalWebSocket, 
  WebSocketOpenEvent, 
  WebSocketMessageEvent, 
  WebSocketCloseEvent, 
  WebSocketErrorEvent 
} from '../shared/WebSocketInterface';

export class ServerWebSocketAdapter implements UniversalWebSocket {
  private nativeSocket: WSWebSocket;

  constructor(url: string) {
    this.nativeSocket = new WSWebSocket(url);
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

  // Consistent addEventListener implementation - adapts EventEmitter to addEventListener
  addEventListener(type: 'open', listener: (event: WebSocketOpenEvent) => void): void;
  addEventListener(type: 'message', listener: (event: WebSocketMessageEvent) => void): void;
  addEventListener(type: 'close', listener: (event: WebSocketCloseEvent) => void): void;
  addEventListener(type: 'error', listener: (event: WebSocketErrorEvent) => void): void;
  addEventListener(type: string, listener: (event: any) => void): void {
    switch (type) {
      case 'open':
        this.nativeSocket.on('open', () => {
          const universalEvent: WebSocketOpenEvent = { type: 'open' };
          listener(universalEvent);
        });
        break;

      case 'message':
        this.nativeSocket.on('message', (data: Buffer | string) => {
          const universalEvent: WebSocketMessageEvent = {
            type: 'message',
            data: data
          };
          listener(universalEvent);
        });
        break;

      case 'close':
        this.nativeSocket.on('close', (code: number, reason: Buffer) => {
          const universalEvent: WebSocketCloseEvent = {
            type: 'close',
            code: code,
            reason: reason.toString()
          };
          listener(universalEvent);
        });
        break;

      case 'error':
        this.nativeSocket.on('error', (error: Error) => {
          const universalEvent: WebSocketErrorEvent = {
            type: 'error',
            error: error,
            message: error.message
          };
          listener(universalEvent);
        });
        break;
    }
  }

  // Consistent removeEventListener implementation - adapts removeListener
  removeEventListener(type: 'open', listener: (event: WebSocketOpenEvent) => void): void;
  removeEventListener(type: 'message', listener: (event: WebSocketMessageEvent) => void): void;
  removeEventListener(type: 'close', listener: (event: WebSocketCloseEvent) => void): void;
  removeEventListener(type: 'error', listener: (event: WebSocketErrorEvent) => void): void;
  removeEventListener(type: string, listener: (event: any) => void): void {
    // Node.js EventEmitter removeListener - simplified implementation
    this.nativeSocket.removeAllListeners(type);
  }
}