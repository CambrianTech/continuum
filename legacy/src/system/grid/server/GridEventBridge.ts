/**
 * GridEventBridge — Bridges Rust grid topology changes to TypeScript Events
 *
 * The Rust grid module tracks node topology (join, leave, health).
 * This bridge subscribes to Rust-side IPC notifications and re-emits them
 * as typed GRID_EVENTS for widget consumption.
 *
 * Also hooks into GridInterceptor to emit routing/forwarding events.
 */

import { Events } from '../../core/shared/Events';
import {
  GRID_EVENTS,
  type GridNodeJoinedEventData,
  type GridNodeLeftEventData,
  type GridNodeHealthChangedEventData,
  type GridRouteDecisionEventData,
  type GridCommandForwardedEventData,
  type GridNodeStatus,
  type GridTransport,
} from '../../events/shared/GridEvents';

/** Cached node states for change detection */
interface NodeState {
  nodeId: string;
  nodeName: string;
  status: GridNodeStatus;
  latencyMs: number;
  lastSeen: number;
}

const nodeStates = new Map<string, NodeState>();

/** Health check interval handle */
let healthCheckInterval: ReturnType<typeof setInterval> | null = null;

/** Node timeout threshold — mark offline after this many ms without a health update */
const NODE_TIMEOUT_MS = 30_000;

/**
 * Process a node health update from Rust IPC.
 * Detects joins, status changes, and emits appropriate events.
 */
export function processNodeHealth(update: {
  nodeId: string;
  nodeName: string;
  status: GridNodeStatus;
  latencyMs: number;
  transport: GridTransport;
  address?: string;
  capabilities?: string[];
  gpu?: { name: string; vramMb: number };
  gpuUtilization?: number;
  gpuMemoryUsedMb?: number;
}): void {
  const existing = nodeStates.get(update.nodeId);
  const now = Date.now();

  if (!existing) {
    // New node — emit join event
    const joinData: GridNodeJoinedEventData = {
      timestamp: now,
      nodeId: update.nodeId,
      nodeName: update.nodeName,
      transport: update.transport,
      address: update.address ?? '',
      capabilities: update.capabilities ?? [],
      gpu: update.gpu,
    };
    Events.emit(GRID_EVENTS.NODE_JOINED, joinData);

    nodeStates.set(update.nodeId, {
      nodeId: update.nodeId,
      nodeName: update.nodeName,
      status: update.status,
      latencyMs: update.latencyMs,
      lastSeen: now,
    });
    return;
  }

  // Existing node — check for status change
  const statusChanged = existing.status !== update.status;
  const latencyChanged = Math.abs(existing.latencyMs - update.latencyMs) > 5;

  if (statusChanged || latencyChanged) {
    const healthData: GridNodeHealthChangedEventData = {
      timestamp: now,
      nodeId: update.nodeId,
      nodeName: update.nodeName,
      status: update.status,
      latencyMs: update.latencyMs,
      previousStatus: statusChanged ? existing.status : undefined,
      gpuUtilization: update.gpuUtilization,
      gpuMemoryUsedMb: update.gpuMemoryUsedMb,
    };
    Events.emit(GRID_EVENTS.NODE_HEALTH_CHANGED, healthData);
  }

  // Update cached state
  existing.status = update.status;
  existing.latencyMs = update.latencyMs;
  existing.lastSeen = now;
}

/**
 * Emit a routing decision event.
 * Called by GridInterceptor when a command is routed.
 */
export function emitRouteDecision(decision: {
  command: string;
  targetNodeId: string;
  targetNodeName: string;
  reason: GridRouteDecisionEventData['reason'];
  candidateCount: number;
}): void {
  const data: GridRouteDecisionEventData = {
    timestamp: Date.now(),
    ...decision,
  };
  Events.emit(GRID_EVENTS.ROUTE_DECISION, data);
}

/**
 * Emit a command forwarded event.
 * Called by GridInterceptor after forwarding completes.
 */
export function emitCommandForwarded(forwarded: {
  command: string;
  targetNodeId: string;
  targetNodeName: string;
  transport: GridTransport;
  durationMs?: number;
  success?: boolean;
  error?: string;
}): void {
  const data: GridCommandForwardedEventData = {
    timestamp: Date.now(),
    ...forwarded,
  };
  Events.emit(GRID_EVENTS.COMMAND_FORWARDED, data);
}

/**
 * Check for timed-out nodes and emit NODE_LEFT events.
 */
function checkNodeTimeouts(): void {
  const now = Date.now();
  for (const [nodeId, state] of nodeStates) {
    if (now - state.lastSeen > NODE_TIMEOUT_MS && state.status !== 'offline') {
      const leftData: GridNodeLeftEventData = {
        timestamp: now,
        nodeId,
        nodeName: state.nodeName,
        reason: 'timeout',
      };
      Events.emit(GRID_EVENTS.NODE_LEFT, leftData);
      nodeStates.delete(nodeId);
    }
  }
}

/**
 * Get current snapshot of all known node states.
 */
export function getNodeStates(): ReadonlyMap<string, Readonly<NodeState>> {
  return nodeStates;
}

/**
 * Get count of nodes by status.
 */
export function getNodeCounts(): { online: number; degraded: number; offline: number; total: number } {
  let online = 0, degraded = 0, offline = 0;
  for (const state of nodeStates.values()) {
    switch (state.status) {
      case 'online': online++; break;
      case 'degraded': degraded++; break;
      case 'offline': offline++; break;
    }
  }
  return { online, degraded, offline, total: nodeStates.size };
}

/**
 * Initialize the GridEventBridge.
 * Subscribes to Rust IPC grid notifications and starts health checking.
 */
export function initializeGridEventBridge(): void {
  // Subscribe to Rust-side grid health reports
  Events.subscribe('grid:rust:node-health', (payload: {
    nodeId: string;
    nodeName: string;
    status: GridNodeStatus;
    latencyMs: number;
    transport: GridTransport;
    address?: string;
    capabilities?: string[];
    gpu?: { name: string; vramMb: number };
    gpuUtilization?: number;
    gpuMemoryUsedMb?: number;
  }) => {
    processNodeHealth(payload);
  });

  // Periodic timeout check
  healthCheckInterval = setInterval(checkNodeTimeouts, 10_000);

  console.log('[GridEventBridge] Initialized — listening for grid:rust:node-health');
}

/**
 * Shut down the GridEventBridge.
 */
export function shutdownGridEventBridge(): void {
  if (healthCheckInterval) {
    clearInterval(healthCheckInterval);
    healthCheckInterval = null;
  }
  nodeStates.clear();
}
