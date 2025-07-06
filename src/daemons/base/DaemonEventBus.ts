/**
 * DaemonEventBus - Strongly typed inter-daemon event communication
 * 
 * Enforces proper event protocols between daemons
 */

import { EventEmitter } from 'events';

// Base event interface that all events must extend
export interface BaseEvent {
  timestamp: Date;
}

// Daemon event extends base with source daemon
export interface DaemonEvent extends BaseEvent {
  source: string; // daemon name that emitted the event
}

// WebSocket-specific base event
export interface WebSocketEvent extends DaemonEvent {
  connectionId: string;
}

// Specific WebSocket event types
export interface WebSocketConnectionEvent extends WebSocketEvent {
  metadata: {
    userAgent: string;
    url: string;
    headers: Record<string, string>;
  };
}

export interface WebSocketDisconnectionEvent extends WebSocketEvent {
  reason?: string;
}

// Session-specific base event
export interface SessionEvent extends DaemonEvent {
  sessionId: string;
}

// Define all valid daemon events with their payloads
export interface DaemonEvents {
  // Session events
  'session_created': {
    sessionId: string;
    sessionType: string;
    owner: string;
    serverLogPath?: string;
  };
  
  'session_joined': {
    sessionId: string;
    sessionType: string;
    owner: string;
    source: string;
  };
  
  'session_stopped': {
    sessionId: string;
    reason: string;
  };
  
  // WebSocket connection events
  'websocket:connection_established': WebSocketConnectionEvent;
  'websocket:connection_closed': WebSocketDisconnectionEvent;
  
  // Browser events
  'browser_launch_requested': {
    sessionId: string;
    url: string;
  };
  
  'browser_launched': {
    sessionId: string;
    browserPid: number;
    url: string;
  };
  
  'browser_connected': {
    sessionId: string;
    connectionId: string;
    userAgent: string;
  };
  
  // Command events
  'command:start': {
    command: string;
    executionId: string;
    timestamp: Date;
  };
  
  'command:complete': {
    command: string;
    executionId: string;
    result: unknown;
    duration: number;
  };
  
  'command:error': {
    command: string;
    executionId: string;
    error: string;
  };
}

// Type-safe event emitter
export class DaemonEventBus extends EventEmitter {
  private static instance: DaemonEventBus;
  
  private constructor() {
    super();
    this.setMaxListeners(100); // Support many daemons
  }
  
  static getInstance(): DaemonEventBus {
    if (!DaemonEventBus.instance) {
      DaemonEventBus.instance = new DaemonEventBus();
    }
    return DaemonEventBus.instance;
  }
  
  // Type-safe emit
  emitEvent<K extends keyof DaemonEvents>(event: K, payload: DaemonEvents[K]): void {
    this.emit(event, payload);
  }
  
  // Type-safe listener
  onEvent<K extends keyof DaemonEvents>(
    event: K,
    listener: (payload: DaemonEvents[K]) => void
  ): void {
    this.on(event, listener);
  }
  
  // Type-safe once listener
  onceEvent<K extends keyof DaemonEvents>(
    event: K,
    listener: (payload: DaemonEvents[K]) => void
  ): void {
    this.once(event, listener);
  }
  
  // Type-safe remove listener
  offEvent<K extends keyof DaemonEvents>(
    event: K,
    listener: (payload: DaemonEvents[K]) => void
  ): void {
    this.off(event, listener);
  }
}

// Export singleton instance
export const DAEMON_EVENT_BUS = DaemonEventBus.getInstance();