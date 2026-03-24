/**
 * Grid Status Command - Server Implementation
 *
 * Routes to Rust GridModule via continuum-core IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridStatusParams, GridStatusResult } from '../shared/GridStatusTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class GridStatusServerCommand extends CommandBase<GridStatusParams, GridStatusResult> {
	private rustClient: RustCoreIPCClient;

	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('grid/status', context, subpath, commander);
		this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
	}

	async execute(_params: GridStatusParams): Promise<GridStatusResult> {
		await this.rustClient.connect();
		const status = await this.rustClient.gridStatus();
		return status as unknown as GridStatusResult;
	}
}
