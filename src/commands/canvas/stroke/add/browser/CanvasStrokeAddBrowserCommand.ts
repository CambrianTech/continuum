/**
 * Canvas Stroke Add Browser Command
 *
 * Delegates to server for persistence and real-time sync.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import type { CanvasStrokeAddParams, CanvasStrokeAddResult } from '../shared/CanvasStrokeAddTypes';

export class CanvasStrokeAddBrowserCommand extends CommandBase<CanvasStrokeAddParams, CanvasStrokeAddResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('canvas/stroke/add', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<CanvasStrokeAddResult> {
    // Always delegate to server for database persistence and event emission
    return this.remoteExecute(params as CanvasStrokeAddParams);
  }
}
