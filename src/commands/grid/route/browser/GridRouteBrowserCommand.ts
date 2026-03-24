/**
 * Grid Route Command - Browser Implementation
 *
 * Browser delegates to server via Commands.execute() (WebSocket transport).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridRouteParams, GridRouteResult } from '../shared/GridRouteTypes';

export class GridRouteBrowserCommand extends CommandBase<GridRouteParams, GridRouteResult> {
	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('grid/route', context, subpath, commander);
	}

	async execute(params: GridRouteParams): Promise<GridRouteResult> {
		return await this.remoteExecute(params);
	}
}
