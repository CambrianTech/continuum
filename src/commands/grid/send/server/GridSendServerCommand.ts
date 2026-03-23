/**
 * Grid Send Command - Server Implementation
 *
 * Routes to Rust GridModule via continuum-core IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridSendParams, GridSendResult } from '../shared/GridSendTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class GridSendServerCommand extends CommandBase<GridSendParams, GridSendResult> {
	private rustClient: RustCoreIPCClient;

	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('grid/send', context, subpath, commander);
		this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
	}

	async execute(params: GridSendParams): Promise<GridSendResult> {
		await this.rustClient.connect();
		const result = await this.rustClient.gridSend(params.nodeId, params.remoteCommand, params.params);
		return { result } as unknown as GridSendResult;
	}
}
