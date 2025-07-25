import { GetChatHistoryCommand } from '../shared/GetChatHistoryCommand';
import { GetChatHistoryParams, GetChatHistoryResult } from '../shared/GetChatHistoryTypes';
import type { ICommandDaemon } from '../../../../shared/CommandBase';
import type { JTAGContext } from '../../../../../../shared/JTAGTypes';

export class GetChatHistoryBrowserCommand extends GetChatHistoryCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  async execute(params: GetChatHistoryParams): Promise<GetChatHistoryResult> {
    try {
      console.log(`📜 BROWSER: Getting chat history for room ${params.roomId}`);

      // Route to server for actual history retrieval
      const result = await this.commander.routeToServer({
        command: 'chat/get-chat-history',
        params: params
      });

      // Update local chat widget with history if applicable
      if (result.success && this.isChatWidgetActive()) {
        await this.updateChatWidgetHistory(result.messages, params);
      }

      return result;
    } catch (error: any) {
      console.error(`❌ BROWSER: Failed to get chat history:`, error);
      return new GetChatHistoryResult({
        roomId: params.roomId,
        requesterId: params.requesterId,
        messages: [],
        totalCount: 0,
        hasMoreMessages: false,
        success: false,
        error: error.message
      });
    }
  }

  private isChatWidgetActive(): boolean {
    return document.querySelector('[data-widget-type="chat"]') !== null;
  }

  private async updateChatWidgetHistory(messages: any[], params: GetChatHistoryParams): Promise<void> {
    const chatWidgets = document.querySelectorAll('[data-widget-type="chat"]');
    
    for (const widget of chatWidgets) {
      const widgetRoomId = widget.getAttribute('data-room-id');
      if (widgetRoomId === params.roomId) {
        const historyEvent = new CustomEvent('chat-history-loaded', {
          detail: {
            roomId: params.roomId,
            messages: messages,
            historyParams: params
          }
        });
        
        widget.dispatchEvent(historyEvent);
      }
    }
  }
}