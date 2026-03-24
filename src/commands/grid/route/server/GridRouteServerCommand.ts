/**
 * Grid Route Command - Server Implementation
 *
 * Routes to Rust GridModule via continuum-core IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridRouteParams, GridRouteResult } from '../shared/GridRouteTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class GridRouteServerCommand extends CommandBase<GridRouteParams, GridRouteResult> {
	private rustClient: RustCoreIPCClient;

	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('grid/route', context, subpath, commander);
		this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
	}

	async execute(params: GridRouteParams): Promise<GridRouteResult> {
		await this.rustClient.connect();
		const result = await this.rustClient.gridRoute(params.targetCommand, params.routingHint);
		return result as unknown as GridRouteResult;
	}
}
