/**
 * Network Types - WebSocket and API communication
 * 
 * Shared types for client-server communication
 */

import { UUID } from '../core/UserPersona';

export enum MessageType {
  COMMAND = 'command',
  RESPONSE = 'response',
  EVENT = 'event',
  HEARTBEAT = 'heartbeat',
  ERROR = 'error'
}

export interface WebSocketMessage {
  readonly type: MessageType;
  readonly id: UUID;
  readonly timestamp: number;
  readonly data: unknown;
  readonly metadata?: Record<string, unknown>;
}

export interface CommandMessage extends WebSocketMessage {
  readonly type: MessageType.COMMAND;
  readonly data: {
    readonly command: string;
    readonly params: Record<string, unknown>;
    readonly sessionId?: UUID;
    readonly userPersonaId?: UUID;
    readonly requestId: UUID;
  };
}

export interface ResponseMessage extends WebSocketMessage {
  readonly type: MessageType.RESPONSE;
  readonly data: {
    readonly requestId: UUID;
    readonly success: boolean;
    readonly result?: unknown;
    readonly error?: string;
    readonly timestamp: number;
  };
}

export interface EventMessage extends WebSocketMessage {
  readonly type: MessageType.EVENT;
  readonly data: {
    readonly eventType: string;
    readonly payload: Record<string, unknown>;
    readonly source: string;
    readonly targetUserPersona?: UUID;
    readonly targetRoom?: UUID;
  };
}

export interface APIResponse<T = unknown> {
  readonly success: boolean;
  readonly data?: T;
  readonly error?: string;
  readonly timestamp: number;
  readonly requestId?: UUID;
  readonly metadata?: Record<string, unknown>;
}