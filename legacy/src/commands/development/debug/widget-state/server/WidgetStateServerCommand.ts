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
    // Log ALL params to debug what's actually received
    console.log('🧠 WidgetStateDebug: Received params keys:', Object.keys(params).filter(k => k !== 'context'));
    console.log('🧠 WidgetStateDebug: Received params:', JSON.stringify({
      hasSetContext: !!params.setContext,
      hasSetRAGString: !!params.setRAGString,
      hasGetStoredContext: !!params.getStoredContext,
      contextSessionId: params.contextSessionId,
      widgetSelector: params.widgetSelector
    }));

    try {
      // Handle context bridging if setContext is provided (legacy format)
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

      // Handle RAG string bridging (new unified state system from PositronicBridge)
      if (params.setRAGString) {
        const sessionId = params.contextSessionId || this.context.uuid;
        console.log(`🧠 WidgetStateDebug: Storing RAG string for session ${sessionId} (${params.setRAGString.length} chars)`);
        console.log(`🧠 WidgetStateDebug: RAG string content preview: ${params.setRAGString.slice(0, 100)}...`);

        // Store pre-formatted RAG string in WidgetContextService
        WidgetContextService.setRAGString(sessionId, params.setRAGString);

        return createWidgetStateDebugResult(this.context, this.context.uuid, {
          success: true,
          widgetFound: true,
          widgetPath: 'rag-string-bridge',
          widgetType: 'positronic-context',
          methods: [],
          state: { properties: { ragStringStored: true, sessionId, ragStringLength: params.setRAGString.length } },
          messages: [],
          debugging: {
            logs: [`Stored RAG string (${params.setRAGString.length} chars)`],
            warnings: [],
            errors: []
          }
        });
      }

      // Handle getStoredContext query - return current server-side context
      if (params.getStoredContext) {
        console.log('🧠 WidgetStateDebug: Querying stored widget context');
        console.log(`🧠 WidgetStateDebug: Query from session ${this.context.uuid}`);

        // Ensure service is initialized
        WidgetContextService.initialize();

        const stats = WidgetContextService.getStats();
        const ragString = WidgetContextService.toRAGContext();
        const rawContext = WidgetContextService.getMostRecentContext();

        return createWidgetStateDebugResult(this.context, this.context.uuid, {
          success: true,
          widgetFound: !!ragString || !!rawContext,
          widgetPath: 'context-query',
          widgetType: rawContext?.widget?.widgetType || (ragString ? 'positronic-context' : 'none'),
          methods: [],
          state: { properties: {} },
          messages: [],
          storedContext: {
            ragString,
            rawContext,
            stats
          },
          debugging: {
            logs: [
              `Active contexts: ${stats.activeContexts}`,
              `Active RAG strings: ${stats.activeRagStrings}`,
              `Service initialized: ${stats.initialized}`,
              ragString ? `RAG string available (${ragString.length} chars)` : 'No RAG string stored',
              rawContext ? `Current widget: ${rawContext.widget.widgetType}` : 'No legacy context stored'
            ],
            warnings: (!ragString && !rawContext) ? ['No widget context stored - widgets may not be emitting state'] : [],
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