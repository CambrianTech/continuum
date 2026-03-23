/**
 * Grid Status Command - Browser Implementation
 *
 * Browser delegates to server via Commands.execute() (WebSocket transport).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridStatusParams, GridStatusResult } from '../shared/GridStatusTypes';

export class GridStatusBrowserCommand extends CommandBase<GridStatusParams, GridStatusResult> {
	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('grid/status', context, subpath, commander);
	}

	async execute(_params: GridStatusParams): Promise<GridStatusResult> {
		return {} as GridStatusResult;
	}
}
