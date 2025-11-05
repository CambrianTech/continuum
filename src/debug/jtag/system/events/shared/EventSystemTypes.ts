/**
 * Event System Types - Comprehensive type definitions for cross-environment events
 * 
 * Centralizes all event-related types to ensure consistency across browser/server environments.
 * Used by EventsDaemon, EventManager, and all event-related components.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { JTAGPayload } from '../../core/types/JTAGTypes';
import type { BaseResponsePayload } from '../../core/types/ResponseTypes';
import { 
  EVENT_SCOPES, 
  EVENT_METADATA_KEYS, 
  COMMON_EVENT_NAMES,
  type EventScope, 
  type CommonEventName,
  type EventMetadataKey 
} from './EventSystemConstants';

/**
 * Base event data structure with bridge metadata
 */
export interface EventData {
  message?: unknown;
  [EVENT_METADATA_KEYS.BRIDGED]?: boolean;
  [EVENT_METADATA_KEYS.ORIGINAL_CONTEXT]?: string;
  [EVENT_METADATA_KEYS.BRIDGE_TIMESTAMP]?: string;
  [EVENT_METADATA_KEYS.BRIDGE_HOP_COUNT]?: number;
  [key: string]: unknown;
}

/**
 * Event scope definition for targeting - required fields prevent failures
 */
export interface EventScopeDefinition {
  type: EventScope;
  id: string;        // Required - Room ID, user ID, etc.
  sessionId: string; // Required - Session UUID for targeting
}

/**
 * Event bridge message payload - strongly typed with required fields
 */
export interface EventBridgePayload extends JTAGPayload {
  type: 'event-bridge';
  scope: EventScopeDefinition;
  eventName: string;
  data: EventData;
  originSessionId: UUID;
  originContextUUID: UUID; // Required - no optional context
  timestamp: string;
}

/**
 * Event bridge response payload - required fields prevent failures
 */
export interface EventBridgeResponse extends BaseResponsePayload {
  bridged: boolean;           // Required - must indicate success/failure
  eventName: string;          // Required - must specify event
  scope: string;              // Required - must specify scope
  routedEnvironments: string[]; // Required - must list where routed
  processingTimeMs: number;   // Required - performance tracking
}

/**
 * Chat-specific event payloads - required fields prevent failures
 */
export interface ChatMessageEventData extends EventData {
  messageId: string;
  roomId: string;
  senderName: string;
  message: string;
  timestamp: string;
  metadata: Record<string, unknown>; // Required - no optional metadata
}

export interface ChatParticipantEventData extends EventData {
  participantId: string;
  roomId: string;
  participantName: string;
  timestamp: string;
}

/**
 * System event payloads
 */
export interface SystemStatusEventData extends EventData {
  status: 'healthy' | 'degraded' | 'critical';
  component: string;
  details: Record<string, unknown>;
  timestamp: string;
}

export interface SystemHealthEventData extends EventData {
  healthScore: number;
  issues: string[];
  timestamp: string;
}

/**
 * Event listener function types
 */
export type EventListenerFunction<T = EventData> = (eventData: T) => void | Promise<void>;

export type ChatMessageListener = EventListenerFunction<ChatMessageEventData>;
export type ChatParticipantListener = EventListenerFunction<ChatParticipantEventData>;
export type SystemStatusListener = EventListenerFunction<SystemStatusEventData>;
export type SystemHealthListener = EventListenerFunction<SystemHealthEventData>;

/**
 * Event emission utilities with strong typing
 */
export interface EventEmissionOptions {
  scope?: EventScopeDefinition;
  bridgeToOtherEnvironments?: boolean;
  priority?: 'CRITICAL' | 'HIGH' | 'NORMAL' | 'LOW';
  metadata?: Record<string, unknown>;
}

/**
 * Event manager interface with strong typing
 */
export interface TypedEventManager {
  // Chat events
  onChatMessage(listener: ChatMessageListener): void;
  onChatParticipantJoined(listener: ChatParticipantListener): void;
  onChatParticipantLeft(listener: ChatParticipantListener): void;
  
  // System events
  onSystemStatusChange(listener: SystemStatusListener): void;
  onSystemHealthUpdate(listener: SystemHealthListener): void;
  
  // Generic event handling
  on(eventName: string, listener: EventListenerFunction): void;
  emit(eventName: string, data: EventData, options?: EventEmissionOptions): void;
  off(eventName: string, listener: EventListenerFunction): void;
}

/**
 * Event routing metadata for debugging and monitoring
 */
export interface EventRoutingMetadata {
  routeStartTime: number;
  sourceEnvironment: 'browser' | 'server';
  targetEnvironments: string[];
  hopCount: number;
  routingPath: string[];
  success: boolean;
  errorMessage?: string;
}

/**
 * Event system health metrics
 */
export interface EventSystemHealth {
  totalEventsProcessed: number;
  crossEnvironmentEvents: number;
  failedRoutings: number;
  averageRoutingTimeMs: number;
  queueSize: number;
  isHealthy: boolean;
  lastHealthCheck: string;
}

/**
 * Event validation utilities
 */
export class EventTypeGuards {
  static isEventBridgePayload(payload: unknown): payload is EventBridgePayload {
    return !!(
      payload &&
      typeof payload === 'object' &&
      'type' in payload &&
      payload.type === 'event-bridge' &&
      'eventName' in payload &&
      'data' in payload &&
      'scope' in payload
    );
  }
  
  static isChatMessageEvent(data: EventData): data is ChatMessageEventData {
    return !!(
      data &&
      'messageId' in data &&
      'roomId' in data &&
      'message' in data
    );
  }
  
  static isSystemStatusEvent(data: EventData): data is SystemStatusEventData {
    return !!(
      data &&
      'status' in data &&
      'component' in data
    );
  }
  
  static hasValidBridgeMetadata(data: EventData): boolean {
    if (!data[EVENT_METADATA_KEYS.BRIDGED]) return false;
    
    const hopCount = data[EVENT_METADATA_KEYS.BRIDGE_HOP_COUNT] || 0;
    return hopCount >= 0 && hopCount <= 10; // Reasonable hop limit
  }
}