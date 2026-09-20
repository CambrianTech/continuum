/**
 * Grid Ping Command - Shared Types
 *
 * Pings a remote node and returns latency and transport info.
 * Routes to Rust GridModule via continuum-core IPC.
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';

export interface GridPingParams extends CommandParams {
	nodeId: string;
}

export interface GridPingResult extends CommandResult {
	nodeId: string;
	nodeName: string | null;
	latencyMs: number;
	transport: string;
	responseType: string;
}
