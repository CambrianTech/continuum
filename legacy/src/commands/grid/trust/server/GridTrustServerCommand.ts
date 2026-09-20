/**
 * Grid Trust Command - Server Implementation
 *
 * Routes to Rust GridModule via continuum-core IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridTrustParams, GridTrustResult } from '../shared/GridTrustTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../../core/continuum-core/bindings/RustCoreIPC';

export class GridTrustServerCommand extends CommandBase<GridTrustParams, GridTrustResult> {
	private rustClient: RustCoreIPCClient;

	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('grid/trust', context, subpath, commander);
		this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
	}

	async execute(params: GridTrustParams): Promise<GridTrustResult> {
		await this.rustClient.connect();
		const result = await this.rustClient.gridTrust(params.nodeId, params.trust);
		return result as unknown as GridTrustResult;
	}
}
