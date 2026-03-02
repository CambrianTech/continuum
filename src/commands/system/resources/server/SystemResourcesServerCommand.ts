/**
 * System Resources Command - Server Implementation
 *
 * Routes to Rust SystemResourceModule via continuum-core IPC:
 * - system/resources: Full CPU + memory snapshot with optional top processes
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { SystemResourcesParams, SystemResourcesResult } from '../shared/SystemResourcesTypes';
import { createSystemResourcesResultFromParams } from '../shared/SystemResourcesTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class SystemResourcesServerCommand extends CommandBase<SystemResourcesParams, SystemResourcesResult> {
	private rustClient: RustCoreIPCClient;

	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('system/resources', context, subpath, commander);
		this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
	}

	async execute(params: SystemResourcesParams): Promise<SystemResourcesResult> {
		await this.rustClient.connect();

		try {
			const snapshot = await this.rustClient.systemResources({
				includeProcesses: params.includeProcesses ?? false,
				topN: params.topN,
			});

			return createSystemResourcesResultFromParams(params, {
				success: true,
				cpu: snapshot.cpu,
				memory: snapshot.memory,
				processes: snapshot.processes,
				timestampMs: snapshot.timestampMs,
				uptimeSeconds: snapshot.uptimeSeconds,
			});
		} finally {
			this.rustClient.disconnect();
		}
	}
}
