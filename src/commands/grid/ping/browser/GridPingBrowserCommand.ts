/**
 * Grid Ping Command - Browser Implementation
 *
 * Browser delegates to server via Commands.execute() (WebSocket transport).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridPingParams, GridPingResult } from '../shared/GridPingTypes';

export class GridPingBrowserCommand extends CommandBase<GridPingParams, GridPingResult> {
	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('grid/ping', context, subpath, commander);
	}

	async execute(_params: GridPingParams): Promise<GridPingResult> {
		return {} as GridPingResult;
	}
}
