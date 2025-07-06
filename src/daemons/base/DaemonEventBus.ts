/**
 * DaemonEventBus - Strongly typed inter-daemon event communication
 * 
 * Enforces proper event protocols between daemons
 */

import { EventEmitter } from 'events';

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
  
  // Browser events
  'browser_launch_requested': {
    sessionId: string;
    url: string;
  };
  
  'browser_launched': {
    sessionId: string;
    pid: number;
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
    result: any;
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