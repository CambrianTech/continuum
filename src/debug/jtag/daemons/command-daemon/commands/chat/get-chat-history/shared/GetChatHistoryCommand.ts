import { CommandBase, type ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { GetChatHistoryParams, type GetChatHistoryResult } from './GetChatHistoryTypes';

export abstract class GetChatHistoryCommand extends CommandBase<GetChatHistoryParams, GetChatHistoryResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('get-chat-history', context, subpath, commander);
  }

  public override getDefaultParams(): GetChatHistoryParams {
    return new GetChatHistoryParams({
      roomId: '',
      participantId: '',
      maxMessages: 50,
      hoursBack: 24,
      includeMetadata: false
    });
  }

  abstract execute(params: GetChatHistoryParams): Promise<GetChatHistoryResult>;
}