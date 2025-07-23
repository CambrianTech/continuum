/**
 * System Event Definitions - Modular event category
 */

export const SystemEvents = {
  READY: 'system.ready',
  SHUTDOWN: 'system.shutdown',
  ERROR: 'system.error',
  HEALTH_UPDATE: 'health.update'
} as const;

export interface SystemEventData {
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