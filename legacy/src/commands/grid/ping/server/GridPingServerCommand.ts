/**
 * Grid Ping Command - Server Implementation
 *
 * Routes to Rust GridModule via continuum-core IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridPingParams, GridPingResult } from '../shared/GridPingTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../../core/continuum-core/bindings/RustCoreIPC';

export class GridPingServerCommand extends CommandBase<GridPingParams, GridPingResult> {
	private rustClient: RustCoreIPCClient;

	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('grid/ping', context, subpath, commander);
		this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
	}

	async execute(params: GridPingParams): Promise<GridPingResult> {
		await this.rustClient.connect();
		const result = await this.rustClient.gridPing(params.nodeId);
		return result as unknown as GridPingResult;
	}
}
