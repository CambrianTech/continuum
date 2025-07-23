/**
 * System Event Definitions - Modular event category
 */

export const SystemEvents = {
  INITIALIZING: 'system.initializing',
  DAEMONS_LOADING: 'system.daemons.loading',
  DAEMONS_LOADED: 'system.daemons.loaded',
  TRANSPORT_READY: 'system.transport.ready',
  READY: 'system.ready',
  SHUTDOWN: 'system.shutdown',
  ERROR: 'system.error',
  HEALTH_UPDATE: 'health.update'
} as const;

export interface SystemEventData {
  [SystemEvents.INITIALIZING]: {
    environment: 'browser' | 'server';
    timestamp: string;
  };
  
  [SystemEvents.DAEMONS_LOADING]: {
    environment: 'browser' | 'server';
    timestamp: string;
    expectedDaemons: string[];
  };
  
  [SystemEvents.DAEMONS_LOADED]: {
    environment: 'browser' | 'server';
    timestamp: string;
    loadedDaemons: string[];
  };
  
  [SystemEvents.TRANSPORT_READY]: {
    environment: 'browser' | 'server';
    timestamp: string;
    transportType: string;
  };
  
  [SystemEvents.READY]: {
    version: string;
    environment: 'browser' | 'server';
    timestamp: string;
    components: string[];
  };
  
  [SystemEvents.SHUTDOWN]: {
    reason: string;
    graceful: boolean;
    uptime: number;
  };
  
  [SystemEvents.ERROR]: {
    error: string;
    component: string;
    recoverable: boolean;
    stack?: string;
  };
  
  [SystemEvents.HEALTH_UPDATE]: {
    status: 'healthy' | 'degraded' | 'unhealthy';
    checks: Record<string, boolean>;
    timestamp: string;
    details?: string;
  };
}

export type SystemEventName = typeof SystemEvents[keyof typeof SystemEvents];