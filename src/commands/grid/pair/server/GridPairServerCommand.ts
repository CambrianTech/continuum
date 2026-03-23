/**
 * Grid Pair Command - Server Implementation
 *
 * Routes to Rust GridModule via continuum-core IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridPairParams, GridPairResult } from '../shared/GridPairTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class GridPairServerCommand extends CommandBase<GridPairParams, GridPairResult> {
	private rustClient: RustCoreIPCClient;

	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('grid/pair', context, subpath, commander);
		this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
	}

	async execute(params: GridPairParams): Promise<GridPairResult> {
		await this.rustClient.connect();
		const result = await this.rustClient.gridPair(params.address, params.name, params.trust);
		return result as unknown as GridPairResult;
	}
}
