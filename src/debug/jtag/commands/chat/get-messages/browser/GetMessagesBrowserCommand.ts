/**
 * Get Messages Command - Browser Implementation
 */

import { GetMessagesCommand } from '../shared/GetMessagesCommand';
import { type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { GetMessagesParams, MessageData } from '../shared/GetMessagesTypes';

export class GetMessagesBrowserCommand extends GetMessagesCommand {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async retrieveMessages(
    params: GetMessagesParams, 
    limit: number
  ): Promise<MessageData[]> {
    try {
      console.log(`📚 Browser: Using remoteExecute for messages in room ${params.roomId}`);
      
      // Browser implementation delegates to server via remoteExecute
      // This maintains environment-agnostic pattern
      const serverResult = await this.remoteExecute<GetMessagesParams, any>({
        ...params,
        limit
      }, 'chat/get-messages', 'server');
      
      if (!serverResult.success || !serverResult.messages) {
        console.warn(`📚 Browser: No messages received from server for room ${params.roomId}`);
        return [];
      }

      console.log(`📚 Browser: Received ${serverResult.messages.length} messages from server`);
      return serverResult.messages;

    } catch (error) {
      console.error(`❌ Browser: Failed to retrieve messages:`, error);
      throw error;
    }
  }

  protected getEnvironmentLabel(): string {
    return 'BROWSER';
  }
}