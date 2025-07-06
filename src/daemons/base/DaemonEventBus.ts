/**
 * DaemonEventBus - Strongly typed inter-daemon event communication
 * 
 * Enforces proper event protocols between daemons
 */

import { EventEmitter } from 'events';
import { 
  SystemEventType,
  WebSocketConnectionEstablishedPayload,
  WebSocketConnectionClosedPayload,
  SessionCreatedPayload,
  SessionJoinedPayload,
  SessionClosedPayload,
  BrowserLaunchedPayload,
  CommandExecutedPayload
} from './EventTypes';

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

// Map event types to their strongly-typed payloads
export interface DaemonEvents {
  // WebSocket events
  [SystemEventType.WEBSOCKET_CONNECTION_ESTABLISHED]: WebSocketConnectionEstablishedPayload;
  [SystemEventType.WEBSOCKET_CONNECTION_CLOSED]: WebSocketConnectionClosedPayload;
  [SystemEventType.WEBSOCKET_MESSAGE_RECEIVED]: {
    connectionId: string;
    message: unknown;
  };
  
  // Session events
  [SystemEventType.SESSION_CREATED]: SessionCreatedPayload;
  [SystemEventType.SESSION_JOINED]: SessionJoinedPayload;
  [SystemEventType.SESSION_CLOSED]: SessionClosedPayload;
  [SystemEventType.SESSION_READY]: {
    sessionId: string;
    connectionId: string;
  };
  
  // Browser events
  [SystemEventType.BROWSER_LAUNCHED]: BrowserLaunchedPayload;
  [SystemEventType.BROWSER_CLOSED]: {
    sessionId: string;
    pid?: number;
  };
  [SystemEventType.BROWSER_CONSOLE_LOG]: {
    sessionId: string;
    level: string;
    message: string;
    timestamp: Date;
  };
  
  // Command events
  [SystemEventType.COMMAND_EXECUTED]: CommandExecutedPayload;
  [SystemEventType.COMMAND_FAILED]: {
    commandName: string;
    sessionId?: string;
    error: string;
  };
  [SystemEventType.COMMAND_QUEUED]: {
    commandName: string;
    sessionId?: string;
    queuePosition: number;
  };
  
  // Daemon events
  [SystemEventType.DAEMON_STARTED]: {
    daemonName: string;
    daemonType: string;
    pid: number;
  };
  [SystemEventType.DAEMON_STOPPED]: {
    daemonName: string;
    reason?: string;
  };
  [SystemEventType.DAEMON_ERROR]: {
    daemonName: string;
    error: string;
    fatal: boolean;
  };
  
  // System events
  [SystemEventType.SYSTEM_STARTUP]: {
    version: string;
    timestamp: Date;
  };
  [SystemEventType.SYSTEM_SHUTDOWN]: {
    reason: string;
    graceful: boolean;
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