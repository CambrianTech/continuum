// ISSUES: 0 open, last updated 2025-07-23 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * System Event Definitions - Core System Event Category
 * 
 * Centralized system-wide event definitions with type-safe data mapping.
 * Provides foundational events for system lifecycle, health monitoring,
 * and cross-component coordination.
 * 
 * CORE ARCHITECTURE:
 * - Const assertion ensures event name type safety
 * - Interface mapping provides compile-time event data validation
 * - Modular event category co-located with system components
 * - ALL_CAPS naming convention for event constants
 * 
 * TESTING REQUIREMENTS:
 * - Unit tests: Event name uniqueness and type mapping correctness
 * - Integration tests: Event emission and handling across system lifecycle
 * - Performance tests: High-frequency health update event handling
 * - Contract tests: Event data structure compatibility across versions
 * 
 * ARCHITECTURAL INSIGHTS:
 * - Event names use dot notation for hierarchical organization
 * - Health events enable distributed system monitoring
 * - Lifecycle events coordinate startup/shutdown sequences
 * - Type union enables exhaustive event handling patterns
 */

import type { JTAGContext } from '../JTAGTypes';

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
    context: JTAGContext;
    timestamp: string;
  };
  
  [SystemEvents.DAEMONS_LOADING]: {
    context: JTAGContext;
    timestamp: string;
    expectedDaemons: string[];
  };
  
  [SystemEvents.DAEMONS_LOADED]: {
    context: JTAGContext;
    timestamp: string;
    loadedDaemons: string[];
  };
  
  [SystemEvents.TRANSPORT_READY]: {
    context: JTAGContext;
    timestamp: string;
    transportType: string;
  };
  
  [SystemEvents.READY]: {
    version: string;
    context: JTAGContext;
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