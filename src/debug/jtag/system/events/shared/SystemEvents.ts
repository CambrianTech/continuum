/**
 * System Event Definitions - Core System Event Category
 *
 * Centralized system-wide event definitions with type-safe data mapping.
 * Provides foundational events for system lifecycle, health monitoring,
 * and cross-component coordination.
 */

import type { JTAGContext } from '../../core/types/JTAGTypes';

export const SYSTEM_EVENTS = {
  INITIALIZING: 'system.initializing',
  DAEMONS_LOADING: 'system.daemons.loading',
  DAEMONS_LOADED: 'system.daemons.loaded',
  TRANSPORT_READY: 'system.transport.ready',
  READY: 'system.ready',
  SHUTDOWN: 'system.shutdown',
  ERROR: 'system.error',
  HEALTH_UPDATE: 'health.update',
} as const;

export interface SystemEventData {
  [SYSTEM_EVENTS.INITIALIZING]: {
    context: JTAGContext;
    timestamp: string;
  };
  [SYSTEM_EVENTS.DAEMONS_LOADING]: {
    context: JTAGContext;
    timestamp: string;
    expectedDaemons: string[];
  };
  [SYSTEM_EVENTS.DAEMONS_LOADED]: {
    context: JTAGContext;
    timestamp: string;
    loadedDaemons: string[];
  };
  [SYSTEM_EVENTS.TRANSPORT_READY]: {
    context: JTAGContext;
    timestamp: string;
    transportType: string;
  };
  [SYSTEM_EVENTS.READY]: {
    version: string;
    context: JTAGContext;
    timestamp: string;
    components: string[];
  };
  [SYSTEM_EVENTS.SHUTDOWN]: {
    reason: string;
    graceful: boolean;
    uptime: number;
  };
  [SYSTEM_EVENTS.ERROR]: {
    error: string;
    component: string;
    recoverable: boolean;
    stack?: string;
  };
  [SYSTEM_EVENTS.HEALTH_UPDATE]: {
    status: 'healthy' | 'degraded' | 'unhealthy';
    checks: Record<string, boolean>;
    timestamp: string;
    details?: string;
  };
}

export type SystemEventName = typeof SYSTEM_EVENTS[keyof typeof SYSTEM_EVENTS];