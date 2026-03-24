/**
 * Grid Status Command - Shared Types
 *
 * Returns Grid transport status, connected nodes, and local identity.
 * Routes to Rust GridModule via continuum-core IPC.
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';

export interface GridStatusParams extends CommandParams {
}

export interface GridStatusResult extends CommandResult {
	transports: Array<{
		name: string;
		connected: boolean;
		address: string | null;
		encrypted: boolean;
	}>;
	totalNodes: number;
	onlineNodes: number;
	gridDir: string;
}
