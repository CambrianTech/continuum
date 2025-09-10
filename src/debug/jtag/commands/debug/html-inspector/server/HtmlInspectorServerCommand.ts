/**
 * HTML Inspector Server Command - Passes through to browser
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { HtmlInspectorParams, HtmlInspectorResult } from '../shared/HtmlInspectorTypes';
import { createHtmlInspectorResult } from '../shared/HtmlInspectorTypes';

export class HtmlInspectorServerCommand extends CommandBase<HtmlInspectorParams, HtmlInspectorResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('html-inspector', context, subpath, commander);
  }
  
  async execute(params: HtmlInspectorParams): Promise<HtmlInspectorResult> {
    // HTML inspection must happen in browser, redirect there
    return createHtmlInspectorResult(params.context, params.sessionId, {
      success: false,
      error: 'HTML inspection must be run in browser environment'
    });
  }
}