/**
 * Grid Send Command - Shared Types
 *
 * Executes a command on a remote node via the Grid mesh.
 * Routes to Rust GridModule via continuum-core IPC.
 *
 * Usage:
 *   ./jtag grid/send --nodeId="100.124.122.107" --remoteCommand="ping"
 *   ./jtag grid/send --nodeId="100.124.122.107" --remoteCommand="genome/layers" --params='{"personaId":"abc"}'
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';

export interface GridSendParams extends CommandParams {
	/** Target node ID (IP or mesh address) */
	nodeId: string;
	/** Command to execute on the remote node */
	remoteCommand: string;
	/** Optional parameters to pass to the remote command */
	params?: Record<string, unknown>;
}

export interface GridSendResult extends CommandResult {
	/** Whether the remote command succeeded */
	success: boolean;
	/** The remote node's response data (unwrapped from transport envelope) */
	remoteResult: unknown;
	/** The node that executed the command */
	nodeId: string;
	/** The command that was executed */
	remoteCommand: string;
}
