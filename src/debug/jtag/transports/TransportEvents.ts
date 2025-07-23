/**
 * Transport Event Definitions - Modular event category
 */

export const TransportEvents = {
  READY: 'transport.ready',
  CONNECTED: 'transport.connected',
  DISCONNECTED: 'transport.disconnected',
  ERROR: 'transport.error'
} as const;

export interface TransportEventData {
  [TransportEvents.READY]: {
    transportType: 'websocket' | 'http';
    port?: number;
    url?: string;
    clientCount?: number;
  };
  
  [TransportEvents.CONNECTED]: {
    transportType: 'websocket' | 'http';
    clientId: string;
    remoteAddress?: string;
  };
  
  [TransportEvents.DISCONNECTED]: {
    transportType: 'websocket' | 'http';
    clientId: string;
    reason: string;
  };
  
  [TransportEvents.ERROR]: {
    transportType: 'websocket' | 'http';
    error: string;
    clientId?: string;
  };
}

export type TransportEventName = typeof TransportEvents[keyof typeof TransportEvents];