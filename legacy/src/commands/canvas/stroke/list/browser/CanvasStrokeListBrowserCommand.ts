/**
 * Canvas Stroke List Browser Command
 *
 * Delegates to server for database query.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import type { CanvasStrokeListParams, CanvasStrokeListResult } from '../shared/CanvasStrokeListTypes';

export class CanvasStrokeListBrowserCommand extends CommandBase<CanvasStrokeListParams, CanvasStrokeListResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('canvas/stroke/list', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<CanvasStrokeListResult> {
    // Always delegate to server for database query
    return this.remoteExecute(params as CanvasStrokeListParams);
  }
}
