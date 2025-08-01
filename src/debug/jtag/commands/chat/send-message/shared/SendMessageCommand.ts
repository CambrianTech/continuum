import { ChatCommandBase } from '@commandsChat/shared/ChatCommandBase';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { type SendMessageParams, type SendMessageResult, createSendMessageParams } from '@chatSendMessage/shared/SendMessageTypes';

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