import { CommandBase, type ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { UUID } from '@shared/CrossPlatformUUID';
import { type GetChatHistoryParams, type GetChatHistoryResult, createGetChatHistoryParams } from '@chatGetChatHistory/shared/GetChatHistoryTypes';

export abstract class GetChatHistoryCommand extends CommandBase<GetChatHistoryParams, GetChatHistoryResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('get-chat-history', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): GetChatHistoryParams {
    return createGetChatHistoryParams(this.context, sessionId, {
      roomId: '',
      participantId: '',
      maxMessages: 50,
      hoursBack: 24,
      includeMetadata: false
    });
  }

  abstract execute(params: GetChatHistoryParams): Promise<GetChatHistoryResult>;
}