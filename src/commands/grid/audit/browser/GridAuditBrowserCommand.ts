/**
 * Grid Audit Command - Browser Implementation
 *
 * Browser delegates to server via Commands.execute() (WebSocket transport).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GridAuditParams, GridAuditResult } from '../shared/GridAuditTypes';

export class GridAuditBrowserCommand extends CommandBase<GridAuditParams, GridAuditResult> {
	constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
		super('grid/audit', context, subpath, commander);
	}

	async execute(_params: GridAuditParams): Promise<GridAuditResult> {
		return {} as GridAuditResult;
	}
}
