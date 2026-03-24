/**
 * Grid Route Command - Shared Types
 *
 * Dry-run routing check: determines whether a command would run locally or
 * be forwarded to a remote node, and why.
 * Routes to Rust GridModule via continuum-core IPC.
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';

export interface GridRouteParams extends CommandParams {
	targetCommand: string;
	routingHint?: string;
}

export interface GridRouteResult extends CommandResult {
	route: 'local' | 'remote';
	nodeId?: string;
	nodeName?: string | null;
	reason: string;
}
