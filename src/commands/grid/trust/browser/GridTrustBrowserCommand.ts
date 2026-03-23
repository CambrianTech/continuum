/**
 * Grid Trust Command - Browser Implementation
 *
 * Browser delegates to server via Commands.execute() (WebSocket transport).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridTrustParams, GridTrustResult } from '../shared/GridTrustTypes';

export class GridTrustBrowserCommand extends CommandBase<GridTrustParams, GridTrustResult> {
	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('grid/trust', context, subpath, commander);
	}

	async execute(_params: GridTrustParams): Promise<GridTrustResult> {
		return {} as GridTrustResult;
	}
}
