/**
 * JTAG WebSocket Message Types - Strict typing for our specific protocol
 * 
 * We control both ends of the communication, so we can be very specific
 * about what messages we send/receive instead of using 'any'.
 */

import type { JTAGMessage } from '../../../core/types/JTAGTypes';
import { TypeUtilities } from '../../../core/types/TypeUtilities';

// All WebSocket transport uses JTAGMessage - no need for separate type

// JTAG-specific WebSocket events with proper typing
export interface JTAGWebSocketOpenEvent {
  type: 'open';
}

export interface JTAGWebSocketMessageEvent {
  type: 'message';
  data: string; // Raw WebSocket data - gets parsed to JTAGMessage
}

export interface JTAGWebSocketCloseEvent {
  type: 'close';
  code: number;
  reason: string;
}

export interface JTAGWebSocketErrorEvent {
  type: 'error';
  error?: Error;
  message?: string;
}

// Session handshake is just a regular JTAG message - no special types needed

// JTAG WebSocket Event Types - constant enum for type safety
export const JTAG_WEBSOCKET_EVENTS = {
  OPEN: 'open',
  MESSAGE: 'message', 
  CLOSE: 'close',
  ERROR: 'error'
} as const;

export type JTAGWebSocketEventType = typeof JTAG_WEBSOCKET_EVENTS[keyof typeof JTAG_WEBSOCKET_EVENTS];

// JTAG WebSocket Ready States - using standard constants
export const JTAG_WEBSOCKET_READY_STATE = {
  CONNECTING: 0,
  OPEN: 1,
  CLOSING: 2,
  CLOSED: 3
} as const;

export type JTAGWebSocketReadyState = typeof JTAG_WEBSOCKET_READY_STATE[keyof typeof JTAG_WEBSOCKET_READY_STATE];

// Type guard functions - no 'any' types, proper unknown handling
export function isJTAGSessionHandshake(message: unknown): boolean {
  return isJTAGMessage(message) && message.endpoint === 'session/handshake';
}

export function isJTAGMessage(message: unknown): message is JTAGMessage {
  if (!TypeUtilities.hasProperties(message, ['endpoint', 'messageType'])) {
    return false;
  }
  
  return typeof message.endpoint === 'string' &&
         (message.messageType === 'event' || message.messageType === 'request' || message.messageType === 'response');
}