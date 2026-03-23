/**
 * Grid Discover Command - Browser Implementation
 *
 * Browser delegates to server via Commands.execute() (WebSocket transport).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridDiscoverParams, GridDiscoverResult } from '../shared/GridDiscoverTypes';

export class GridDiscoverBrowserCommand extends CommandBase<GridDiscoverParams, GridDiscoverResult> {
	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('grid/discover', context, subpath, commander);
	}

	async execute(_params: GridDiscoverParams): Promise<GridDiscoverResult> {
		return {} as GridDiscoverResult;
	}
}
