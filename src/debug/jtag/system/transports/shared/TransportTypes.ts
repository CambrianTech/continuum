/**
 * Transport Types - Shared interfaces and types for transport system
 * 
 * Extracted from TransportFactory megafile into proper modular structure.
 * Provides core transport interfaces that all transport implementations must follow.
 */

import type { JTAGMessage } from '@shared/JTAGTypes';
import type { EventsInterface } from '@shared/JTAGEventSystem';

/**
 * Transport configuration interface
 */
export interface TransportConfig {
  preferred?: 'websocket' | 'http' | 'udp-multicast';
  fallback?: boolean;
  serverPort?: number;
  serverUrl?: string;
  eventSystem?: EventsInterface;
  sessionId?: string; // Session ID for client handshake
  // UDP multicast specific options
  p2p?: {
    nodeId?: string;
    nodeType?: 'server' | 'browser' | 'mobile' | 'ai-agent';
    capabilities?: string[];
    multicastAddress?: string;
    multicastPort?: number;
    unicastPort?: number;
    encryptionKey?: string;
  };
}

/**
 * Transport send result interface
 */
export interface TransportSendResult {
  success: boolean;
  timestamp: string;
  sentCount?: number;
}

/**
 * JTAG Transport Interface
 * 
 * Abstraction for cross-context message delivery mechanisms.
 * Implementations include WebSocket, HTTP, and UDP multicast transports.
 */
export interface JTAGTransport {
  name: string;
  send(message: JTAGMessage): Promise<TransportSendResult>;
  isConnected(): boolean;
  disconnect(): Promise<void>;
  reconnect?(): Promise<void>;
  setMessageHandler?(handler: (message: JTAGMessage) => void): void;
}