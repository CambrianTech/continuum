/**
 * Chat Export Command - Browser Implementation
 * Delegates to server
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { ChatExportCommand } from '../shared/ChatExportCommand';
import type { ChatExportParams, ChatExportResult } from '../shared/ChatExportTypes';

export class ChatExportBrowserCommand extends ChatExportCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeChatExport(params: ChatExportParams): Promise<ChatExportResult> {
    // Delegate to server
    return await this.remoteExecute(params);
  }
}
