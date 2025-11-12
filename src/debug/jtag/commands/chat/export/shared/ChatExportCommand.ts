/**
 * Chat Export Command - Shared Implementation
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ChatExportParams, ChatExportResult } from './ChatExportTypes';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';

export abstract class ChatExportCommand extends CommandBase<ChatExportParams, ChatExportResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('chat/export', context, subpath, commander);
  }

  /**
   * Server handles the actual export logic
   * Browser delegates to server
   */
  protected abstract executeChatExport(params: ChatExportParams): Promise<ChatExportResult>;

  async execute(params: JTAGPayload): Promise<ChatExportResult> {
    return this.executeChatExport(params as ChatExportParams);
  }
}
