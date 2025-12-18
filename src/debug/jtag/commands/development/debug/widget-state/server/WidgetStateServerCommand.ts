/**
 * Widget State Debug Server Command
 *
 * Routes widget state inspection to browser environment where widgets exist
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { WidgetStateDebugParams, WidgetStateDebugResult } from '../shared/WidgetStateDebugTypes';
import { createWidgetStateDebugResult } from '../shared/WidgetStateDebugTypes';

export class WidgetStateServerCommand extends CommandBase<WidgetStateDebugParams, WidgetStateDebugResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('widget-state-debug', context, subpath, commander);
  }

  async execute(params: WidgetStateDebugParams): Promise<WidgetStateDebugResult> {
    try {
      // Route to browser where widgets exist
      console.log(`🔍 WidgetStateDebug: Routing to browser for widget inspection: ${params.widgetSelector || 'chat-widget'}`);

      const browserResult = await this.remoteExecute<WidgetStateDebugParams, WidgetStateDebugResult>(params);

      if (browserResult.success) {
        console.log(`✅ WidgetStateDebug: Successfully inspected ${params.widgetSelector || 'chat-widget'}`);
        return browserResult;
      } else {
        console.error(`❌ WidgetStateDebug: Browser inspection failed: ${browserResult.error}`);
        return browserResult;
      }

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ WidgetStateDebug: Server command failed: ${errorMessage}`);

      return createWidgetStateDebugResult(this.context, this.context.uuid, {
        success: false,
        error: `Widget state debugging must be run in browser environment: ${errorMessage}`,
        debugging: {
          logs: [`Attempted to route to browser for widget inspection`],
          warnings: [],
          errors: [errorMessage]
        }
      });
    }
  }
}