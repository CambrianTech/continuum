/**
 * Grid Setup Check Command - Server Implementation
 *
 * Routes to Rust GridModule via continuum-core IPC.
 * Returns comprehensive diagnostics for grid/Tailscale setup.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridSetupCheckParams, GridSetupCheckResult } from '../shared/GridSetupCheckTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../../core/continuum-core/bindings/RustCoreIPC';

export class GridSetupCheckServerCommand extends CommandBase<GridSetupCheckParams, GridSetupCheckResult> {
	private rustClient: RustCoreIPCClient;

	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('grid/setup-check', context, subpath, commander);
		this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
	}

	async execute(_params: GridSetupCheckParams): Promise<GridSetupCheckResult> {
		await this.rustClient.connect();
		const result = await this.rustClient.gridSetupCheck();
		return result as unknown as GridSetupCheckResult;
	}
}
