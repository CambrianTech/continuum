/**
 * Transport Event Definitions - Modular event category
 */

export const TRANSPORT_EVENTS = {
  READY: 'transport.ready',
  CONNECTED: 'transport.connected',
  DISCONNECTED: 'transport.disconnected',
  ERROR: 'transport.error'
} as const;

export interface TransportEventData {
  [TRANSPORT_EVENTS.READY]: {
    transportType: 'websocket' | 'http';
    port?: number;
    url?: string;
    clientCount?: number;
  };
  
  [TRANSPORT_EVENTS.CONNECTED]: {
    transportType: 'websocket' | 'http';
    clientId: string;
    remoteAddress?: string;
  };
  
  [TRANSPORT_EVENTS.DISCONNECTED]: {
    transportType: 'websocket' | 'http';
    clientId: string;
    reason: string;
  };
  
  [TRANSPORT_EVENTS.ERROR]: {
    transportType: 'websocket' | 'http';
    error: string;
    clientId?: string;
  };
}

export type TransportEventName = typeof TRANSPORT_EVENTS[keyof typeof TRANSPORT_EVENTS];