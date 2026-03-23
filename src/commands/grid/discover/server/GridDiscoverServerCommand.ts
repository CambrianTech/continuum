/**
 * Grid Discover Command - Server Implementation
 *
 * Routes to Rust GridModule via continuum-core IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridDiscoverParams, GridDiscoverResult } from '../shared/GridDiscoverTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class GridDiscoverServerCommand extends CommandBase<GridDiscoverParams, GridDiscoverResult> {
	private rustClient: RustCoreIPCClient;

	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('grid/discover', context, subpath, commander);
		this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
	}

	async execute(_params: GridDiscoverParams): Promise<GridDiscoverResult> {
		await this.rustClient.connect();
		const result = await this.rustClient.gridDiscover();
		return result as unknown as GridDiscoverResult;
	}
}
