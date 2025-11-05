/**
 * Connection Types - WebSocket client connection interfaces
 */

import WebSocket from 'ws';

export interface Client {
  readonly id: string;
  readonly socket: WebSocket;
  connected: boolean;
  readonly connectTime: Date;
  lastActivity: Date;
  metadata: Record<string, any>;
}

export interface ConnectionConfig {
  readonly maxClients?: number;
  readonly heartbeatInterval?: number;
  readonly clientTimeout?: number;
  readonly enableHeartbeat?: boolean;
  readonly enableAuth?: boolean;
  readonly authTimeout?: number;
}

export interface ConnectionStats {
  readonly totalClients: number;
  readonly maxClients: number;
  readonly averageConnectionTime: number;
  readonly oldestConnection: number | null;
  readonly heartbeatEnabled: boolean;
  readonly heartbeatInterval: number;
}

export interface ServerConfig {
  readonly port?: number;
  readonly host?: string;
  readonly maxClients?: number;
  readonly enableHeartbeat?: boolean;
  readonly enableAuth?: boolean;
  readonly daemonConfig?: DaemonConfig;
}

export interface DaemonConfig {
  readonly autoConnect?: boolean;
  readonly enableFallback?: boolean;
  readonly retryAttempts?: number;
  readonly retryInterval?: number;
}