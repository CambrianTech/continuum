/**
 * Server WebSocket Adapter - Makes Node.js ws WebSocket conform to JTAGUniversalWebSocket interface
 * 
 * Adapts Node.js ws WebSocket to use consistent addEventListener pattern with JTAG-specific typing.
 */

import { WebSocket as WSWebSocket } from 'ws';
import type { 
  JTAGUniversalWebSocket, 
  JTAGWebSocketOpenEvent, 
  JTAGWebSocketMessageEvent, 
  JTAGWebSocketCloseEvent, 
  JTAGWebSocketErrorEvent,
  JTAGWebSocketReadyState
} from '../shared/WebSocketInterface';
// No longer need JTAGWebSocketMessageData - everything is JTAGMessage

export class ServerWebSocketAdapter implements JTAGUniversalWebSocket {
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

  get readyState(): JTAGWebSocketReadyState {
    return this.nativeSocket.readyState as JTAGWebSocketReadyState;
  }

  get url(): string {
    return this.nativeSocket.url;
  }

  // Consistent addEventListener implementation - adapts EventEmitter to addEventListener with JTAG typing
  addEventListener(type: 'open', listener: (event: JTAGWebSocketOpenEvent) => void): void;
  addEventListener(type: 'message', listener: (event: JTAGWebSocketMessageEvent) => void): void;
  addEventListener(type: 'close', listener: (event: JTAGWebSocketCloseEvent) => void): void;
  addEventListener(type: 'error', listener: (event: JTAGWebSocketErrorEvent) => void): void;
  addEventListener(type: string, listener: (event: any) => void): void {
    switch (type) {
      case 'open':
        this.nativeSocket.on('open', () => {
          const jtagEvent: JTAGWebSocketOpenEvent = { type: 'open' };
          listener(jtagEvent);
        });
        break;

      case 'message':
        this.nativeSocket.on('message', (data: Buffer | string) => {
          // Convert all data to string first to match JTAGWebSocketMessageData type
          const stringData: string = Buffer.isBuffer(data) ? data.toString('utf8') : (data as string);
          const jtagEvent: JTAGWebSocketMessageEvent = {
            type: 'message',
            data: stringData
          };
          listener(jtagEvent);
        });
        break;

      case 'close':
        this.nativeSocket.on('close', (code: number, reason: Buffer) => {
          const jtagEvent: JTAGWebSocketCloseEvent = {
            type: 'close',
            code: code,
            reason: reason.toString()
          };
          listener(jtagEvent);
        });
        break;

      case 'error':
        this.nativeSocket.on('error', (error: Error) => {
          const jtagEvent: JTAGWebSocketErrorEvent = {
            type: 'error',
            error: error,
            message: error.message
          };
          listener(jtagEvent);
        });
        break;
    }
  }

  // Consistent removeEventListener implementation - adapts removeListener with JTAG typing
  removeEventListener(type: 'open', listener: (event: JTAGWebSocketOpenEvent) => void): void;
  removeEventListener(type: 'message', listener: (event: JTAGWebSocketMessageEvent) => void): void;
  removeEventListener(type: 'close', listener: (event: JTAGWebSocketCloseEvent) => void): void;
  removeEventListener(type: 'error', listener: (event: JTAGWebSocketErrorEvent) => void): void;
  removeEventListener(type: string, listener: (event: any) => void): void {
    // Node.js EventEmitter removeListener - simplified implementation
    this.nativeSocket.removeAllListeners(type);
  }
}