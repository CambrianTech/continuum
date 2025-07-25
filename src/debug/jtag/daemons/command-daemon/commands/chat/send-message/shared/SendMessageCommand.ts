import { ChatCommandBase } from '@chatShared/ChatCommandBase';
import type { ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { SendMessageParams, type SendMessageResult } from './SendMessageTypes';

export abstract class SendMessageCommand extends ChatCommandBase<SendMessageParams, SendMessageResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('send-message', context, subpath, commander);
  }

  public override getDefaultParams(): SendMessageParams {
    return new SendMessageParams({
      roomId: '',
      content: '',
      senderId: '',
      messageType: 'text'
    });
  }

  abstract execute(params: SendMessageParams): Promise<SendMessageResult>;
}