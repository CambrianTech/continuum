/**
 * Grid Pair Command - Browser Implementation
 *
 * Browser delegates to server via Commands.execute() (WebSocket transport).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridPairParams, GridPairResult } from '../shared/GridPairTypes';

export class GridPairBrowserCommand extends CommandBase<GridPairParams, GridPairResult> {
	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('grid/pair', context, subpath, commander);
	}

	async execute(_params: GridPairParams): Promise<GridPairResult> {
		return {} as GridPairResult;
	}
}
