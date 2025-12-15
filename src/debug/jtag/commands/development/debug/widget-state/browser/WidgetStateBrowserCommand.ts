/**
 * Widget State Debug Browser Command
 *
 * Clean, modular widget introspection using reusable utilities
 * Addresses DEBUG-FRICTION.md gaps with proper architecture
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { WidgetStateDebugParams, WidgetStateDebugResult } from '../shared/WidgetStateDebugTypes';
import { createWidgetStateDebugResult } from '../shared/WidgetStateDebugTypes';
import {
  WidgetDiscovery,
  WidgetAnalyzer,
  WidgetDOMAnalyzer,
  WidgetConnectivityTester
} from '@system/core/browser/utils/WidgetIntrospection';

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

      // Filter out heavy properties to prevent JSON truncation
      const filteredProperties = { ...widgetState.properties };

      // Always exclude templateCSS (huge)
      if (filteredProperties.templateCSS) {
        delete filteredProperties.templateCSS;
      }

      // Exclude messages array - use messageCount instead
      if (Array.isArray(filteredProperties.messages)) {
        const messageCount = filteredProperties.messages.length;
        delete filteredProperties.messages;
        filteredProperties.messageCount = messageCount;
        logs.push(`📨 Excluded ${messageCount} messages from properties to prevent JSON truncation`);
      }

      widgetState.properties = filteredProperties;

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

      // Get dimensions and computed styles
      let dimensions;
      if (params.includeDimensions) {
        const element = widgetRef.element;
        const rect = element.getBoundingClientRect();
        const computed = window.getComputedStyle(element);
        dimensions = {
          width: Math.round(rect.width),
          height: Math.round(rect.height),
          scrollHeight: element.scrollHeight,
          clientHeight: element.clientHeight,
          hasScrollbar: element.scrollHeight > element.clientHeight,
          computedHeight: computed.height,
          computedDisplay: computed.display,
          computedFlex: computed.flex,
          computedOverflow: computed.overflow
        };
        logs.push(`📐 Dimensions: ${dimensions.width}x${dimensions.height}, scrollable: ${dimensions.hasScrollbar}`);
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

      // Extract entity COUNT and IDs from scroller
      // Full entities are too large, but we need IDs for test verification
      let entityCount = 0;
      let entityIds: string[] = [];
      const widget = widgetRef.element as any;
      if (widget.scroller && typeof widget.scroller === 'object') {
        // entities might be a getter function or array property
        const entitiesValue = typeof widget.scroller.entities === 'function'
          ? widget.scroller.entities()
          : widget.scroller.entities;

        if (Array.isArray(entitiesValue)) {
          entityCount = entitiesValue.length;
          // Extract just the IDs for verification (lightweight)
          // Try multiple ID property names (id, _id, entityId, etc.)
          entityIds = entitiesValue.map((e: any) => e?.id || e?._id || e?.entityId).filter(id => id !== undefined && id !== null);
          logs.push(`📦 Found ${entityCount} entities in widget scroller (extracted ${entityIds.length} IDs for verification)`);
          if (entityIds.length === 0 && entityCount > 0) {
            // DEBUG: Log first entity structure to see what properties it has
            const firstEntity = entitiesValue[0];
            const keys = firstEntity ? Object.keys(firstEntity).slice(0, 10) : [];
            logs.push(`⚠️ Entity ID extraction failed - first entity keys: ${keys.join(', ')}`);
          }
        } else {
          logs.push(`⚠️ Widget has scroller but entities is not an array: ${typeof entitiesValue}`);
        }
      } else {
        logs.push(`⚠️ Widget does not have scroller or scroller is not an object: ${typeof widget.scroller}`);
      }

      // If countOnly, return minimal response with just entityCount
      if (params.countOnly) {
        return createWidgetStateDebugResult(this.context, this.context.uuid, {
          success: true,
          widgetFound: true,
          widgetPath: widgetRef.path,
          widgetType: widgetRef.type,
          methods: [],
          state: {
            properties: {},
            entityCount,
          },
          messages: [],
          connectivity: WidgetConnectivityTester.testDataConnectivity(widgetRef.element),
          debugging: { logs, warnings, errors }
        });
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
          entityCount,
          entityIds, // Include entity IDs for test verification
        },
        messages,
        eventSystem,
        domInfo,
        dimensions,
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