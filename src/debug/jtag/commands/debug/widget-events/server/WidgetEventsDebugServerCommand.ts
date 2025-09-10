/**
 * Widget Events Debug Server Command - Passes through to browser
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { WidgetEventsDebugParams, WidgetEventsDebugResult } from '../shared/WidgetEventsDebugTypes';
import { createWidgetEventsDebugResult } from '../shared/WidgetEventsDebugTypes';

export class WidgetEventsDebugServerCommand extends CommandBase<WidgetEventsDebugParams, WidgetEventsDebugResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('widget-events', context, subpath, commander);
  }
  
  async execute(params: WidgetEventsDebugParams): Promise<WidgetEventsDebugResult> {
    // Widget events debugging must happen in browser, redirect there
    return createWidgetEventsDebugResult(this.context, params.sessionId || 'unknown', {
      success: false,
      error: 'Widget events debugging must be run in browser environment'
    });
  }
}