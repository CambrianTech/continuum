/**
 * Widget State Debug Server Command
 *
 * Routes widget state inspection to browser environment where widgets exist
 * Also handles context bridging for RAG awareness
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { WidgetStateDebugParams, WidgetStateDebugResult } from '../shared/WidgetStateDebugTypes';
import { createWidgetStateDebugResult } from '../shared/WidgetStateDebugTypes';
import { WidgetContextService } from '@system/rag/services/WidgetContextService';
import type { PositronicContext } from '../../../../../widgets/shared/services/state/PositronWidgetState';

export class WidgetStateServerCommand extends CommandBase<WidgetStateDebugParams, WidgetStateDebugResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('widget-state-debug', context, subpath, commander);
  }

  async execute(params: WidgetStateDebugParams): Promise<WidgetStateDebugResult> {
    try {
      // Handle context bridging if setContext is provided
      if (params.setContext) {
        const sessionId = params.contextSessionId || this.context.uuid;
        console.log(`🧠 WidgetStateDebug: Storing widget context for session ${sessionId.slice(0, 8)}`);

        // Convert to PositronicContext format
        const context: PositronicContext = {
          widget: {
            widgetType: params.setContext.widget.widgetType,
            section: params.setContext.widget.section,
            title: params.setContext.widget.title,
            metadata: params.setContext.widget.metadata,
            timestamp: params.setContext.widget.timestamp || Date.now()
          },
          interaction: params.setContext.interaction,
          breadcrumb: params.setContext.breadcrumb,
          dwellTimeMs: params.setContext.dwellTimeMs
        };

        // Store in WidgetContextService for RAG pipeline
        WidgetContextService.setContext(sessionId, context);

        return createWidgetStateDebugResult(this.context, this.context.uuid, {
          success: true,
          widgetFound: true,
          widgetPath: 'context-bridge',
          widgetType: params.setContext.widget.widgetType,
          methods: [],
          state: { properties: { contextStored: true, sessionId } },
          messages: [],
          debugging: {
            logs: [`Stored widget context for RAG: ${params.setContext.widget.widgetType}`],
            warnings: [],
            errors: []
          }
        });
      }

      // Route to browser where widgets exist (original behavior)
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