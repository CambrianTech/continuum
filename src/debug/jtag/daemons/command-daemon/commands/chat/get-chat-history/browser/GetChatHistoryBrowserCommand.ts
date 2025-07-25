import { GetChatHistoryCommand } from '../shared/GetChatHistoryCommand';
import { GetChatHistoryParams, GetChatHistoryResult, type ChatMessage } from '../shared/GetChatHistoryTypes';
import type { ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';

export class GetChatHistoryBrowserCommand extends GetChatHistoryCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  public override async execute(params: GetChatHistoryParams): Promise<GetChatHistoryResult> {
    const { roomId } = params;
    
    try {
      console.log(`📜 BROWSER: Getting chat history for room ${roomId}`);

      // Route to server for actual history retrieval
      const result = await this.remoteExecute(params);

      // Update local chat widget with history if applicable
      if (result.success && this.isChatWidgetActive()) {
        await this.updateChatWidgetHistory([...result.messages], params);
      }

      return result;
    } catch (error: any) {
      console.error(`❌ BROWSER: Failed to get chat history:`, error);
      
      // Elegant error result with spread operator
      return new GetChatHistoryResult({
        ...this.createBaseResult(),
        success: false,
        roomId: roomId ?? '',
        messages: [],
        totalCount: 0,
        error: error?.message ?? String(error)
      });
    }
  }

  private isChatWidgetActive(): boolean {
    return document.querySelector('[data-widget-type="chat"]') !== null;
  }

  private async updateChatWidgetHistory(messages: ChatMessage[], params: GetChatHistoryParams): Promise<void> {
    const { roomId } = params;
    const chatWidgets = [...document.querySelectorAll('[data-widget-type="chat"]')];
    
    // Elegant filtering and event dispatch with spread
    chatWidgets
      .filter(widget => widget.getAttribute('data-room-id') === roomId)
      .forEach(widget => {
        const historyEvent = new CustomEvent('chat-history-loaded', {
          detail: {
            roomId,
            messages: [...messages], // Defensive copy with spread
            historyParams: { ...params } // Spread params for immutability
          }
        });
        
        widget.dispatchEvent(historyEvent);
      });
  }

  /**
   * Create base result properties with spread pattern
   */
  private createBaseResult() {
    return {
      environment: this.context.environment,
      timestamp: new Date().toISOString()
    };
  }
}