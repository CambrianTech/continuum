/**
 * Grid Events — Real-time topology and routing events for the Grid system
 *
 * These events provide visibility into the multi-node Grid network:
 * - Node join/leave events for topology changes
 * - Health updates for latency/status monitoring
 * - Routing decisions for command delegation visibility
 * - Command forwarding notifications
 */

/**
 * Grid Event Types
 * Emitted by GridEventBridge from Rust grid state changes
 */
export const GRID_EVENTS = {
  /** A new node joined the grid */
  NODE_JOINED: 'grid:node:joined',

  /** A node left the grid (disconnected or timed out) */
  NODE_LEFT: 'grid:node:left',

  /** Node health metrics changed (latency, status) */
  NODE_HEALTH_CHANGED: 'grid:node:health-changed',

  /** A routing decision was made for a command */
  ROUTE_DECISION: 'grid:route:decision',

  /** A command was forwarded to a remote node */
  COMMAND_FORWARDED: 'grid:command:forwarded',
} as const;

export type GridEventType = typeof GRID_EVENTS[keyof typeof GRID_EVENTS];

// ── Event Data Types ──────────────────────────────────────────────────────

/** Base event data for all grid events */
export interface GridEventData {
  /** Timestamp of the event */
  timestamp: number;
}

/** Transport type used to reach the node */
export type GridTransport = 'tailscale' | 'reticulum' | 'local';

/** Node status */
export type GridNodeStatus = 'online' | 'degraded' | 'offline';

/** Event data for NODE_JOINED */
export interface GridNodeJoinedEventData extends GridEventData {
  /** Unique node ID */
  nodeId: string;

  /** Human-readable node name */
  nodeName: string;

  /** Transport used to connect */
  transport: GridTransport;

  /** Node's IP or mesh address */
  address: string;

  /** Capabilities advertised by the node */
  capabilities: string[];

  /** GPU info if available */
  gpu?: {
    name: string;
    vramMb: number;
  };
}

/** Event data for NODE_LEFT */
export interface GridNodeLeftEventData extends GridEventData {
  nodeId: string;
  nodeName: string;

  /** Reason for leaving */
  reason: 'disconnect' | 'timeout' | 'error' | 'shutdown';
}

/** Event data for NODE_HEALTH_CHANGED */
export interface GridNodeHealthChangedEventData extends GridEventData {
  nodeId: string;
  nodeName: string;

  /** Current status */
  status: GridNodeStatus;

  /** Round-trip latency in ms */
  latencyMs: number;

  /** Previous status (for transition detection) */
  previousStatus?: GridNodeStatus;

  /** GPU utilization 0-100 if reported */
  gpuUtilization?: number;

  /** GPU memory used in MB if reported */
  gpuMemoryUsedMb?: number;
}

/** Event data for ROUTE_DECISION */
export interface GridRouteDecisionEventData extends GridEventData {
  /** Command that was routed */
  command: string;

  /** Node selected for execution */
  targetNodeId: string;
  targetNodeName: string;

  /** Why this node was selected */
  reason: 'local' | 'capability' | 'load-balance' | 'explicit' | 'gpu-required';

  /** Other candidates considered */
  candidateCount: number;
}

/** Event data for COMMAND_FORWARDED */
export interface GridCommandForwardedEventData extends GridEventData {
  /** Command name */
  command: string;

  /** Target node */
  targetNodeId: string;
  targetNodeName: string;

  /** Transport used */
  transport: GridTransport;

  /** Execution time in ms (set on completion) */
  durationMs?: number;

  /** Whether the forwarded command succeeded */
  success?: boolean;

  /** Error message if failed */
  error?: string;
}
