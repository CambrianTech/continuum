/**
 * Widget Interact Server Command
 *
 * Routes widget interaction to browser environment - all DOM operations happen browser-side
 * Server provides the command interface and handles result processing
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { WidgetInteractParams, WidgetInteractResult } from '../shared/WidgetInteractTypes';
import { createWidgetInteractResult } from '../shared/WidgetInteractTypes';

export class WidgetInteractServerCommand extends CommandBase<WidgetInteractParams, WidgetInteractResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/debug/widget-interact', context, subpath, commander);
  }

  async execute(params: WidgetInteractParams): Promise<WidgetInteractResult> {
    const logs: string[] = [];
    const warnings: string[] = [];
    const errors: string[] = [];

    try {
      logs.push(`🎯 Server routing widget interaction: ${params.action} on ${params.widgetSelector}`);

      // Route to browser for DOM interaction
      const browserResult = await this.remoteExecute(
        params,
        'widget-interact',
        'browser'
      ) as WidgetInteractResult;

      if (browserResult.success) {
        logs.push(`✅ Browser interaction completed successfully`);
        logs.push(`🔄 Action: ${browserResult.action}, Widget: ${browserResult.widgetPath}`);

        if (browserResult.actionResult?.executed) {
          logs.push(`📄 Result: ${browserResult.actionResult.returnValue || 'Action executed'}`);
        }

        if (browserResult.screenshots) {
          logs.push(`📸 Screenshots: before=${!!browserResult.screenshots.before}, after=${!!browserResult.screenshots.after}`);
        }

        if (browserResult.verification?.performed) {
          logs.push(`🔍 Verification: ${browserResult.verification.success ? 'PASSED' : 'FAILED'} - ${browserResult.verification.details}`);
        }

        // Merge debugging info
        logs.push(...(browserResult.debugging?.logs || []));
        warnings.push(...(browserResult.debugging?.warnings || []));
        errors.push(...(browserResult.debugging?.errors || []));

        return createWidgetInteractResult(this.context, this.context.uuid, {
          ...browserResult,
          debugging: { logs, warnings, errors }
        });
      } else {
        errors.push(`❌ Browser interaction failed: ${browserResult.error || 'Unknown error'}`);
        errors.push(...(browserResult.debugging?.errors || []));
        warnings.push(...(browserResult.debugging?.warnings || []));

        return createWidgetInteractResult(this.context, this.context.uuid, {
          success: false,
          action: params.action,
          widgetFound: browserResult.widgetFound || false,
          widgetPath: browserResult.widgetPath || `${params.widgetSelector} interaction failed`,
          error: browserResult.error || 'Browser interaction failed',
          debugging: { logs, warnings, errors }
        });
      }

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      errors.push(`❌ Server routing failed: ${errorMessage}`);

      return createWidgetInteractResult(this.context, this.context.uuid, {
        success: false,
        action: params.action,
        error: `Server routing error: ${errorMessage}`,
        debugging: { logs, warnings, errors }
      });
    }
  }
}