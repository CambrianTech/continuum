/**
 * Router Event Definitions - Co-located with Router module
 */

export const ROUTER_EVENTS = {
  INITIALIZED: 'router.initialized',
  MESSAGE_SENT: 'router.message-sent',
  MESSAGE_RECEIVED: 'router.message-received',
  ROUTE_FAILED: 'router.route-failed'
} as const;

export interface RouterEventData {
  [ROUTER_EVENTS.INITIALIZED]: {
    environment: 'browser' | 'server';
    transportCount: number;
  };
  
  [ROUTER_EVENTS.MESSAGE_SENT]: {
    messageId: string;
    origin: string;
    endpoint: string;
    payloadType: string;
  };
  
  [ROUTER_EVENTS.MESSAGE_RECEIVED]: {
    messageId: string;
    origin: string;
    endpoint: string;
    payloadType: string;
  };
  
  [ROUTER_EVENTS.ROUTE_FAILED]: {
    messageId: string;
    endpoint: string;
    error: string;
  };
}

export type RouterEventName = typeof ROUTER_EVENTS[keyof typeof ROUTER_EVENTS];