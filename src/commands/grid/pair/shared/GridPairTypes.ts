/**
 * Grid Pair Command - Shared Types
 *
 * Pairs with a remote node by address, optionally assigning name and trust level.
 * Routes to Rust GridModule via continuum-core IPC.
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';

export interface GridPairParams extends CommandParams {
	address: string;
	name?: string;
	trust?: string;
}

export interface GridPairResult extends CommandResult {
	paired: boolean;
	nodeId: string;
	trustLevel: string;
}
