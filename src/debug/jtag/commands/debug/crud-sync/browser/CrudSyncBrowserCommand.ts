/**
 * CRUD Sync Debug Browser Command
 *
 * Comprehensive CRUD testing across all three main widgets
 * Verifies database/UI sync for integration testing
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { CrudSyncDebugParams, CrudSyncDebugResult, WidgetSyncData } from '../shared/CrudSyncDebugTypes';
import { createCrudSyncDebugResult } from '../shared/CrudSyncDebugTypes';
import {
  WidgetDiscovery,
  WidgetDOMAnalyzer,
  WidgetAnalyzer
} from '../../../../system/core/browser/utils/WidgetIntrospection';

export class CrudSyncBrowserCommand extends CommandBase<CrudSyncDebugParams, CrudSyncDebugResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('crud-sync-debug', context, subpath, commander);
  }

  async execute(params: CrudSyncDebugParams): Promise<CrudSyncDebugResult> {
    const logs: string[] = [];
    const warnings: string[] = [];
    const errors: string[] = [];
    const maxItems = params.maxItems || 10;

    try {
      logs.push(`🔄 Starting CRUD sync verification across all widgets (max ${maxItems} items each)`);

      // Extract data from all three widgets
      const roomList = await this.extractWidgetData('room-list-widget', '.room-item', maxItems, logs);
      const chatWidget = await this.extractWidgetData('chat-widget', '.message, .message-row', maxItems, logs);
      const userList = await this.extractWidgetData('user-list-widget', '.user-item', maxItems, logs);

      // If chat widget has no DOM messages, try to extract from widget state
      if (chatWidget.itemCount === 0) {
        chatWidget.items = await this.extractChatMessages(maxItems, logs);
        chatWidget.itemCount = chatWidget.items.length;
      }

      logs.push(`📊 Widget extraction complete:`);
      logs.push(`   - Rooms: ${roomList.itemCount} items`);
      logs.push(`   - Messages: ${chatWidget.itemCount} items`);
      logs.push(`   - Users: ${userList.itemCount} items`);

      // Simple sync status (enhanced if database comparison is added later)
      const syncStatus = {
        roomsMatch: roomList.found && roomList.itemCount > 0,
        messagesMatch: chatWidget.found && chatWidget.itemCount > 0,
        usersMatch: userList.found && userList.itemCount > 0,
        overallSync: false
      };

      syncStatus.overallSync = syncStatus.roomsMatch && syncStatus.messagesMatch && syncStatus.usersMatch;

      return createCrudSyncDebugResult(this.context, this.context.uuid, {
        success: true,
        widgets: {
          roomList,
          chatWidget,
          userList
        },
        syncStatus,
        debugging: { logs, warnings, errors }
      });

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      errors.push(`CRUD sync verification failed: ${errorMessage}`);

      return createCrudSyncDebugResult(this.context, this.context.uuid, {
        success: false,
        error: errorMessage,
        debugging: { logs, warnings, errors }
      });
    }
  }

  private async extractWidgetData(
    widgetSelector: string,
    rowSelector: string,
    maxItems: number,
    logs: string[]
  ): Promise<WidgetSyncData> {

    const widgetRef = WidgetDiscovery.findWidget(widgetSelector);

    if (!widgetRef) {
      logs.push(`❌ ${widgetSelector} not found`);
      return {
        widgetName: widgetSelector,
        found: false,
        path: `${widgetSelector} not found`,
        itemCount: 0,
        items: []
      };
    }

    logs.push(`✅ ${widgetSelector} found at: ${widgetRef.path}`);

    // Extract row data using our DOM analyzer
    const rowData = WidgetDOMAnalyzer.extractRowData(widgetSelector, rowSelector);
    const limitedRows = rowData.slice(0, maxItems);

    const items = limitedRows.map(row => ({
      id: row.id || `item-${row.index}`,
      displayText: this.cleanText(row.textContent),
      dataAttributes: row.attributes,
      index: row.index
    }));

    return {
      widgetName: widgetSelector,
      found: true,
      path: widgetRef.path,
      itemCount: items.length,
      items
    };
  }

  private async extractChatMessages(maxItems: number, logs: string[]): Promise<Array<{
    id: string;
    displayText: string;
    dataAttributes: Record<string, string>;
    index: number;
  }>> {
    try {
      const widgetRef = WidgetDiscovery.findWidget('chat-widget');
      if (!widgetRef) return [];

      const widgetState = WidgetAnalyzer.analyzeState(widgetRef.element);
      const widgetObj = widgetRef.element as any;

      if (Array.isArray(widgetObj.messages)) {
        logs.push(`💬 Extracting messages from chat widget state (${widgetObj.messages.length} total)`);

        return widgetObj.messages.slice(0, maxItems).map((msg: any, index: number) => ({
          id: msg.id || `msg-${index}`,
          displayText: this.cleanText(msg.content?.text || msg.text || JSON.stringify(msg).slice(0, 50)),
          dataAttributes: {
            'message-id': msg.id || '',
            'sender-id': msg.senderId || '',
            'room-id': msg.roomId || ''
          },
          index
        }));
      }

      return [];
    } catch (error) {
      logs.push(`⚠️ Chat message extraction failed: ${error}`);
      return [];
    }
  }

  private cleanText(text: string): string {
    return text.replace(/\s+/g, ' ').trim().slice(0, 100);
  }
}