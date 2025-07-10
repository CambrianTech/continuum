/**
 * Session Event Types - Strongly Typed Session Event Payloads
 * 
 * These interfaces are used by both:
 * - Core server daemons (SessionManagerDaemon, BrowserManagerDaemon, etc.)
 * - Browser client daemons (BrowserWebSocketDaemon, BrowserConsoleDaemon, etc.)
 * 
 * This ensures perfect type alignment and prevents session management errors.
 */

import { SessionEvent } from './EventTypes';

/**
 * Base session event interface - all session events must extend this
 */
export interface BaseSessionEvent {
  type: SessionEvent;
  sessionId: string;
  timestamp: string;
  source: 'client' | 'server' | 'system';
}

/**
 * Session Creation Event - When a new session is created
 */
export interface SessionCreatedEvent extends BaseSessionEvent {
  type: SessionEvent.CREATED;
  payload: {
    sessionType: 'development' | 'production' | 'test';
    owner: string;
    config: {
      focus?: boolean;
      killZombies?: boolean;
      forceNew?: boolean;
    };
    metadata: {
      createdAt: string;
      workingDirectory: string;
      version: string;
    };
  };
}

/**
 * Session Join Event - When someone joins an existing session
 */
export interface SessionJoinedEvent extends BaseSessionEvent {
  type: SessionEvent.JOINED;
  payload: {
    sessionType: 'development' | 'production' | 'test';
    owner: string;
    config: {
      focus?: boolean;
      killZombies?: boolean;
      forceNew?: boolean;
    };
    connectionInfo: {
      connectionId: string;
      clientType: 'cli' | 'browser' | 'api';
      userAgent?: string;
    };
    metadata: {
      joinedAt: string;
      existingSession: boolean;
      workingDirectory: string;
    };
  };
}

/**
 * Session Ready Event - When session is fully initialized and ready
 */
export interface SessionReadyEvent extends BaseSessionEvent {
  type: SessionEvent.READY;
  payload: {
    sessionId: string;
    connectionId: string;
    status: 'ready';
    capabilities: {
      browserControl: boolean;
      consoleCapture: boolean;
      commandExecution: boolean;
    };
  };
}

/**
 * Session Update Event - When session data changes
 */
export interface SessionUpdateEvent extends BaseSessionEvent {
  type: SessionEvent.UPDATE;
  payload: {
    changes: Record<string, any>;
    reason: 'user_action' | 'system_update' | 'config_change';
    updatedFields: string[];
  };
}

/**
 * Union type of all session events for type checking
 */
export type SessionEventType = 
  | SessionCreatedEvent
  | SessionJoinedEvent
  | SessionReadyEvent
  | SessionUpdateEvent;

/**
 * Type guard functions to check event types at runtime
 */
export function isSessionCreatedEvent(event: BaseSessionEvent): event is SessionCreatedEvent {
  return event.type === SessionEvent.CREATED;
}

export function isSessionJoinedEvent(event: BaseSessionEvent): event is SessionJoinedEvent {
  return event.type === SessionEvent.JOINED;
}

export function isSessionReadyEvent(event: BaseSessionEvent): event is SessionReadyEvent {
  return event.type === SessionEvent.READY;
}

export function isSessionUpdateEvent(event: BaseSessionEvent): event is SessionUpdateEvent {
  return event.type === SessionEvent.UPDATE;
}

/**
 * Event selector functions with compiler enforcement
 * These functions provide type-safe event selection and prevent typos
 */
export const SessionEventSelectors = {
  /**
   * Select session creation events
   */
  created: (sessionId?: string) => ({
    type: SessionEvent.CREATED,
    ...(sessionId && { sessionId })
  }),
  
  /**
   * Select session join events
   */
  joined: (sessionId?: string) => ({
    type: SessionEvent.JOINED,
    ...(sessionId && { sessionId })
  }),
  
  /**
   * Select session ready events
   */
  ready: (sessionId?: string) => ({
    type: SessionEvent.READY,
    ...(sessionId && { sessionId })
  }),
  
  /**
   * Select all session lifecycle events for a specific session
   */
  allForSession: (sessionId: string) => [
    { type: SessionEvent.CREATED, sessionId },
    { type: SessionEvent.JOINED, sessionId },
    { type: SessionEvent.READY, sessionId },
    { type: SessionEvent.UPDATE, sessionId }
  ]
} as const;

/**
 * Session event factory functions for creating properly typed events
 */
export const SessionEventFactory = {
  /**
   * Create a session created event
   */
  created: (
    sessionId: string,
    sessionType: 'development' | 'production' | 'test',
    owner: string,
    config: SessionCreatedEvent['payload']['config'],
    metadata: SessionCreatedEvent['payload']['metadata']
  ): SessionCreatedEvent => ({
    type: SessionEvent.CREATED,
    sessionId,
    timestamp: new Date().toISOString(),
    source: 'server',
    payload: {
      sessionType,
      owner,
      config,
      metadata
    }
  }),
  
  /**
   * Create a session joined event
   */
  joined: (
    sessionId: string,
    sessionType: 'development' | 'production' | 'test',
    owner: string,
    config: SessionJoinedEvent['payload']['config'],
    connectionInfo: SessionJoinedEvent['payload']['connectionInfo'],
    metadata: SessionJoinedEvent['payload']['metadata']
  ): SessionJoinedEvent => ({
    type: SessionEvent.JOINED,
    sessionId,
    timestamp: new Date().toISOString(),
    source: 'server',
    payload: {
      sessionType,
      owner,
      config,
      connectionInfo,
      metadata
    }
  }),
  
  /**
   * Create a session ready event
   */
  ready: (
    sessionId: string,
    connectionId: string,
    capabilities: SessionReadyEvent['payload']['capabilities']
  ): SessionReadyEvent => ({
    type: SessionEvent.READY,
    sessionId,
    timestamp: new Date().toISOString(),
    source: 'server',
    payload: {
      sessionId,
      connectionId,
      status: 'ready',
      capabilities
    }
  })
} as const;