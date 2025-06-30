/**
 * WebSocket Type Declarations
 * Based on @types/ws official definitions
 */

declare module 'ws' {
  import { EventEmitter } from 'events';
  import { IncomingMessage } from 'http';
  import { Socket } from 'net';

  export interface WebSocketEventMap {
    close: { code: number; reason: Buffer };
    error: { error: Error };
    message: { data: any; isBinary: boolean };
    open: {};
    ping: { data: Buffer };
    pong: { data: Buffer };
  }

  export class WebSocket extends EventEmitter {
    static readonly CONNECTING: 0;
    static readonly OPEN: 1;
    static readonly CLOSING: 2;
    static readonly CLOSED: 3;

    readonly CONNECTING: 0;
    readonly OPEN: 1;
    readonly CLOSING: 2;
    readonly CLOSED: 3;

    readonly readyState: 0 | 1 | 2 | 3;
    readonly protocol: string;
    readonly url: string;

    constructor(address: string | URL, protocols?: string | string[], options?: any);

    send(data: any, options?: any, callback?: (error?: Error) => void): void;
    close(code?: number, reason?: string | Buffer): void;
    ping(data?: any, mask?: boolean, callback?: (error?: Error) => void): void;
    pong(data?: any, mask?: boolean, callback?: (error?: Error) => void): void;

    // Event listeners
    on(event: 'close', listener: (code: number, reason: Buffer) => void): this;
    on(event: 'error', listener: (error: Error) => void): this;
    on(event: 'message', listener: (data: any, isBinary: boolean) => void): this;
    on(event: 'open', listener: () => void): this;
    on(event: 'ping', listener: (data: Buffer) => void): this;
    on(event: 'pong', listener: (data: Buffer) => void): this;
    on(event: string | symbol, listener: (...args: any[]) => void): this;
  }

  export class WebSocketServer extends EventEmitter {
    readonly clients: Set<WebSocket>;

    constructor(options?: {
      port?: number;
      host?: string;
      server?: any;
      path?: string;
    });

    on(event: 'connection', listener: (ws: WebSocket, req: IncomingMessage) => void): this;
    on(event: 'error', listener: (error: Error) => void): this;
    on(event: 'listening', listener: () => void): this;
    on(event: string | symbol, listener: (...args: any[]) => void): this;

    close(callback?: (error?: Error) => void): void;
  }

  export { WebSocket as default };
}