/**
 * Chat Send Message Browser Command
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { ChatSendMessageCommand } from '../shared/ChatSendMessageCommand';

export class ChatSendMessageBrowserCommand extends ChatSendMessageCommand {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected getEnvironmentLabel(): string {
    return 'BROWSER';
  }

  /**
   * Browser-specific event emission (no-op - events handled server-side)
   */
  protected async emitMessageEvent(message: any): Promise<void> {
    // Browser doesn't emit events - server handles event emission
    console.log(`🌐 BROWSER: Message ${message.messageId} will emit events server-side`);
  }
}