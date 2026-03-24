/**
 * Grid Nodes Command - Browser Implementation
 *
 * Browser delegates to server via Commands.execute() (WebSocket transport).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridNodesParams, GridNodesResult } from '../shared/GridNodesTypes';

export class GridNodesBrowserCommand extends CommandBase<GridNodesParams, GridNodesResult> {
	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('grid/nodes', context, subpath, commander);
	}

	async execute(params: GridNodesParams): Promise<GridNodesResult> {
		return await this.remoteExecute(params);
	}
}
