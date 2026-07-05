/**
 * Grid Audit Command - Server Implementation
 *
 * Routes to Rust GridModule via continuum-core IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridAuditParams, GridAuditResult } from '../shared/GridAuditTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../../core/continuum-core/bindings/RustCoreIPC';

export class GridAuditServerCommand extends CommandBase<GridAuditParams, GridAuditResult> {
	private rustClient: RustCoreIPCClient;

	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('grid/audit', context, subpath, commander);
		this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
	}

	async execute(params: GridAuditParams): Promise<GridAuditResult> {
		await this.rustClient.connect();
		const entries = await this.rustClient.gridAudit(params.limit);
		return { entries } as unknown as GridAuditResult;
	}
}
