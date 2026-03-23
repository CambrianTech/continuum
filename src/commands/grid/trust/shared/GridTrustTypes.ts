/**
 * Grid Trust Command - Shared Types
 *
 * Updates the trust level assigned to a known node.
 * Routes to Rust GridModule via continuum-core IPC.
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';

export interface GridTrustParams extends CommandParams {
	nodeId: string;
	trust: string;
}

export interface GridTrustResult extends CommandResult {
	nodeId: string;
	trustLevel: string;
}
