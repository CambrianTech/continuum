/**
 * Grid Send Command - Browser Implementation
 *
 * Browser delegates to server via Commands.execute() (WebSocket transport).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridSendParams, GridSendResult } from '../shared/GridSendTypes';

export class GridSendBrowserCommand extends CommandBase<GridSendParams, GridSendResult> {
	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('grid/send', context, subpath, commander);
	}

	async execute(params: GridSendParams): Promise<GridSendResult> {
		return await this.remoteExecute(params);
	}
}
