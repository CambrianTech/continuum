/**
 * Router Event Definitions - Co-located with Router module
 */

export const RouterEvents = {
  INITIALIZED: 'router.initialized',
  MESSAGE_SENT: 'router.message-sent',
  MESSAGE_RECEIVED: 'router.message-received',
  ROUTE_FAILED: 'router.route-failed'
} as const;

export interface RouterEventData {
  [RouterEvents.INITIALIZED]: {
    environment: 'browser' | 'server';
    transportCount: number;
  };
  
  [RouterEvents.MESSAGE_SENT]: {
    messageId: string;
    origin: string;
    endpoint: string;
    payloadType: string;
  };
  
  [RouterEvents.MESSAGE_RECEIVED]: {
    messageId: string;
    origin: string;
    endpoint: string;
    payloadType: string;
  };
  
  [RouterEvents.ROUTE_FAILED]: {
    messageId: string;
    endpoint: string;
    error: string;
  };
}

export type RouterEventName = typeof RouterEvents[keyof typeof RouterEvents];