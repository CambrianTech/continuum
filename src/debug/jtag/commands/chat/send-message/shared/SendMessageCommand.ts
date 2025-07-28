import { ChatCommandBase } from '@commandsChat/shared/ChatCommandBase';
import type { ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { UUID } from 'crypto';
import { type SendMessageParams, type SendMessageResult, createSendMessageParams } from './SendMessageTypes';

export abstract class SendMessageCommand extends ChatCommandBase<SendMessageParams, SendMessageResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('send-message', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): SendMessageParams {
    return createSendMessageParams(this.context, sessionId, {
      roomId: '',
      content: '',
      senderId: '',
      messageType: 'text'
    });
  }

  abstract execute(params: SendMessageParams): Promise<SendMessageResult>;
}