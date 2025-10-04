/**
 * Widget State Debug Browser Command
 *
 * Clean, modular widget introspection using reusable utilities
 * Addresses DEBUG-FRICTION.md gaps with proper architecture
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { WidgetStateDebugParams, WidgetStateDebugResult } from '../shared/WidgetStateDebugTypes';
import { createWidgetStateDebugResult } from '../shared/WidgetStateDebugTypes';
import {
  WidgetDiscovery,
  WidgetAnalyzer,
  WidgetDOMAnalyzer,
  WidgetConnectivityTester
} from '../../../../system/core/browser/utils/WidgetIntrospection';

export class WidgetStateBrowserCommand extends CommandBase<WidgetStateDebugParams, WidgetStateDebugResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('widget-state-debug', context, subpath, commander);
  }

  async execute(params: WidgetStateDebugParams): Promise<WidgetStateDebugResult> {
    const logs: string[] = [];
    const warnings: string[] = [];
    const errors: string[] = [];

    try {
      const widgetSelector = params.widgetSelector || 'chat-widget';
      logs.push(`🔍 Starting widget introspection: ${widgetSelector}`);

      // Use modular widget discovery
      const widgetRef = WidgetDiscovery.findWidget(widgetSelector);

      if (!widgetRef) {
        return createWidgetStateDebugResult(this.context, this.context.uuid, {
          success: false,
          widgetFound: false,
          widgetPath: `No path - ${widgetSelector} not found`,
          error: `Widget ${widgetSelector} not found in shadow DOM`,
          debugging: { logs, warnings, errors }
        });
      }

      logs.push(`✅ Widget found: ${widgetRef.path}`);

      // Analyze widget state using modular analyzer
      const widgetState = WidgetAnalyzer.analyzeState(widgetRef.element);
      logs.push(`📊 Analyzed ${widgetState.methods.length} methods, ${Object.keys(widgetState.properties).length} properties`);

      // Filter out heavy properties when extracting row data to prevent JSON truncation
      if (params.extractRowData && widgetState.properties.templateCSS) {
        widgetState.properties = { ...widgetState.properties };
        widgetState.properties.templateCSS = '[CSS EXCLUDED FOR ROW EXTRACTION]';
        logs.push(`🎨 CSS excluded to prevent JSON truncation during row extraction`);
      }

      // Extract messages if requested
      let messages: Array<{ id: string; content?: Record<string, unknown>; [key: string]: unknown }> = [];
      if (params.includeMessages) {
        const widgetObj = widgetRef.element as unknown as Record<string, unknown>;
        if (Array.isArray(widgetObj.messages)) {
          messages = widgetObj.messages.slice(0, 10).map((msg: any, index: number) => ({
            id: msg.id || `msg-${index}`,
            content: msg.content || { text: msg.text || JSON.stringify(msg).slice(0, 100) },
            index,
            senderId: msg.senderId,
            roomId: msg.roomId,
            timestamp: msg.timestamp
          }));
          logs.push(`📨 Found ${widgetObj.messages.length} messages (showing first 10 with details)`);
        }
      }

      // Analyze event system if requested
      let eventSystem;
      if (params.includeEventListeners) {
        eventSystem = WidgetAnalyzer.analyzeEventSystem(widgetRef.element);
        logs.push(`🎧 Event system: ${eventSystem.eventEmitterSize} registered events`);
      }

      // Analyze DOM structure if requested
      let domInfo;
      if (params.includeDomInfo) {
        domInfo = WidgetDOMAnalyzer.analyzeDOMStructure(widgetRef.element);
        logs.push(`🌐 DOM: ${domInfo.elementCount} elements, ${domInfo.cssClasses.length} CSS classes`);
      }

      // Extract row data if requested
      let rowData;
      if (params.extractRowData) {
        // Let WidgetDOMAnalyzer use widget-specific selectors unless explicitly overridden
        const rowSelector = params.rowSelector; // undefined means use widget-specific logic
        rowData = WidgetDOMAnalyzer.extractRowData(params.widgetSelector || 'chat-widget', rowSelector);
        const selectorUsed = rowSelector || `widget-specific selectors for ${params.widgetSelector}`;
        logs.push(`📋 Rows: Found ${rowData.length} items with selector "${selectorUsed}"`);
      }

      // Test data connectivity if requested
      let dataTest, connectivity;
      if (params.testDataConnectivity) {
        connectivity = WidgetConnectivityTester.testDataConnectivity(widgetRef.element);
        logs.push(`🔗 Connectivity: commands=${connectivity.canExecuteCommands}, window.jtag=${connectivity.windowJtag}`);

        dataTest = {
          rawData: {
            hasJtagOperation: connectivity.hasJtagOperation,
            hasExecuteCommand: connectivity.hasExecuteCommand
          },
          filteredData: {
            canExecuteCommands: connectivity.canExecuteCommands
          },
          filterResults: [
            connectivity.hasJtagOperation ? 'jtagOperation available' : 'jtagOperation missing',
            connectivity.hasExecuteCommand ? 'executeCommand available' : 'executeCommand missing'
          ]
        };
      } else {
        connectivity = WidgetConnectivityTester.testDataConnectivity(widgetRef.element);
      }

      // Extract entities from scroller - access widget element directly, not analyzed state
      let entities = [];
      const widget = widgetRef.element as any;
      if (widget.scroller && typeof widget.scroller === 'object') {
        // entities might be a getter function or array property
        const entitiesValue = typeof widget.scroller.entities === 'function'
          ? widget.scroller.entities()
          : widget.scroller.entities;

        if (Array.isArray(entitiesValue)) {
          entities = entitiesValue;
          logs.push(`📦 Extracted ${entities.length} entities from widget scroller`);
        } else {
          logs.push(`⚠️ Widget has scroller but entities is not an array: ${typeof entitiesValue}`);
        }
      } else {
        logs.push(`⚠️ Widget does not have scroller or scroller is not an object: ${typeof widget.scroller}`);
      }

      return createWidgetStateDebugResult(this.context, this.context.uuid, {
        success: true,
        widgetFound: true,
        widgetPath: widgetRef.path,
        widgetType: widgetRef.type,
        methods: widgetState.methods.slice(0, 20), // Limit for readability
        state: {
          properties: widgetState.properties,
          messageCount: widgetState.messageCount,
          currentRoomId: widgetState.currentRoomId,
          entities,
        },
        messages,
        eventSystem,
        domInfo,
        rowData,
        dataTest,
        connectivity,
        debugging: { logs, warnings, errors }
      });

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      errors.push(`Widget introspection failed: ${errorMessage}`);

      return createWidgetStateDebugResult(this.context, this.context.uuid, {
        success: false,
        error: errorMessage,
        debugging: { logs, warnings, errors }
      });
    }
  }
}