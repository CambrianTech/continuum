/**
 * Grid Discover Command - Shared Types
 *
 * Triggers node discovery across all transports.
 * Routes to Rust GridModule via continuum-core IPC.
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';

export interface GridDiscoverParams extends CommandParams {
}

export interface GridDiscoverResult extends CommandResult {
	totalDiscovered: number;
	transports: Array<{
		transport: string;
		discovered?: number;
		error?: string;
	}>;
}
