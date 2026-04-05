/**
 * GridDataNormalizer — Normalize Rust snake_case grid responses to camelCase
 *
 * Rust grid module returns snake_case (node_id, node_name, vram_mb).
 * Widgets expect camelCase. This is the SINGLE normalizer — all grid
 * widgets import from here instead of doing inline mapping.
 *
 * If the Rust response format changes, fix it HERE, not in 3 widgets.
 */

import type { GridTransport, GridNodeStatus } from '../../system/events/shared/GridEvents';

/** Status dot colors — single source of truth for all grid widgets */
export const GRID_STATUS_COLORS: Record<GridNodeStatus, string> = {
  online: 'var(--status-online, #00ff88)',
  degraded: 'var(--status-away, #ffaa00)',
  offline: 'var(--status-offline, #666666)',
};

/** Normalized node data for widget consumption */
export interface NormalizedGridNode {
  nodeId: string;
  nodeName: string;
  status: GridNodeStatus;
  latencyMs: number;
  transport: GridTransport;
  address: string;
  capabilities: string[];
  gpu?: { name: string; vramMb: number };
  gpuUtilization?: number;
  gpuMemoryUsedMb?: number;
}

/**
 * Normalize a single node from Rust grid/nodes response.
 * Handles both snake_case (Rust) and camelCase (already normalized) fields.
 */
export function normalizeGridNode(raw: Record<string, any>): NormalizedGridNode {
  const nodeId = raw.node_id ?? raw.nodeId ?? '';
  const nodeName = raw.node_name ?? raw.nodeName ?? nodeId;

  // Capabilities is an array of objects like { type: "compute", gpu: "RTX 5090", vram_mb: 32768 }
  const caps: any[] = raw.capabilities ?? [];
  const gpuCap = caps.find((c: any) => c.gpu);
  const gpu = gpuCap
    ? { name: gpuCap.gpu, vramMb: gpuCap.vram_mb ?? gpuCap.vramMb ?? 0 }
    : raw.gpu;

  // Addresses is an array — take the first one for transport/IP
  const firstAddr = raw.addresses?.[0];
  const transport: GridTransport = firstAddr?.transport ?? raw.transport ?? 'tailscale';
  const address = firstAddr?.ip ?? raw.address ?? '';

  // Capability strings for display
  const capStrings = caps.map((c: any) => {
    if (c.gpu) return `${c.gpu} (${Math.round((c.vram_mb ?? 0) / 1024)}GB)`;
    return c.type ?? String(c);
  });

  return {
    nodeId,
    nodeName,
    status: raw.status ?? 'online',
    latencyMs: raw.latencyMs ?? raw.latency_ms ?? 0,
    transport,
    address,
    capabilities: capStrings,
    gpu,
    gpuUtilization: raw.gpuUtilization ?? raw.gpu_utilization,
    gpuMemoryUsedMb: raw.gpuMemoryUsedMb ?? raw.gpu_memory_used_mb,
  };
}

/**
 * Normalize the full grid/nodes response.
 * Returns empty array if response is invalid.
 */
export function normalizeGridNodes(result: any): NormalizedGridNode[] {
  const nodes = result?.nodes;
  if (!Array.isArray(nodes)) return [];
  return nodes
    .filter((n: any) => (n.trust_level ?? n.trustLevel ?? '') !== 'blocked')
    .map(normalizeGridNode);
}
