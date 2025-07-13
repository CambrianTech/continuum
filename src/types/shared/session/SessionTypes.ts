/**
 * Session Types - Session state and token management
 * 
 * Handles session state, connection tokens, and room membership
 */

import { UUID } from '../core/UserPersona';
import { RoomToken } from '../chat/ChatTypes';

export enum SessionStatus {
  CONNECTING = 'connecting',
  CONNECTED = 'connected',
  RECONNECTING = 'reconnecting',
  DISCONNECTED = 'disconnected',
  ERROR = 'error'
}

export interface SessionState {
  readonly sessionId: UUID;
  readonly userPersonaId: UUID;
  readonly status: SessionStatus;
  readonly connectedAt: Date;
  readonly lastActivity: Date;
  
  // Room connections
  readonly activeRooms: Set<UUID>;
  readonly currentRoom?: UUID;
  readonly roomTokens: Map<UUID, RoomToken>;
  
  // Connection state
  readonly clientId?: string;
  readonly connectionAttempts: number;
  readonly isReconnecting: boolean;
  
  // Capabilities in this session
  readonly capabilities: string[];
  readonly permissions: string[];
}

export interface ConnectionToken {
  readonly tokenId: UUID;
  readonly sessionId: UUID;
  readonly userPersonaId: UUID;
  readonly issuedAt: Date;
  readonly expiresAt: Date;
  readonly scope: string[];
  readonly metadata: Record<string, unknown>;
}

export interface SessionConfiguration {
  readonly sessionType: 'development' | 'production' | 'testing';
  readonly owner: 'user' | 'shared' | 'system';
  readonly autoReconnect: boolean;
  readonly maxReconnectAttempts: number;
  readonly heartbeatInterval: number;
  readonly tokenRefreshThreshold: number;
}