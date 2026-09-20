/**
 * Grid Nodes Command - Server Implementation
 *
 * Routes to Rust GridModule via continuum-core IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridNodesParams, GridNodesResult } from '../shared/GridNodesTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../../core/continuum-core/bindings/RustCoreIPC';

export class GridNodesServerCommand extends CommandBase<GridNodesParams, GridNodesResult> {
	private rustClient: RustCoreIPCClient;

	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('grid/nodes', context, subpath, commander);
		this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
	}

	async execute(_params: GridNodesParams): Promise<GridNodesResult> {
		await this.rustClient.connect();
		const nodes = await this.rustClient.gridNodes();
		return { nodes } as unknown as GridNodesResult;
	}
}
