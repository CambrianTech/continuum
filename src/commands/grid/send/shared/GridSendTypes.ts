/**
 * Grid Send Command - Shared Types
 *
 * Executes a command on a remote node via the Grid mesh.
 * Routes to Rust GridModule via continuum-core IPC.
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';

export interface GridSendParams extends CommandParams {
	nodeId: string;
	remoteCommand: string;
	params?: Record<string, unknown>;
}

export interface GridSendResult extends CommandResult {
	result: unknown;
}
