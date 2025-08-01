import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { SendMessageParams } from './SendMessageTypes';
import type { SendMessageResult } from './SendMessageTypes';

export abstract class SendMessageCommand extends CommandBase<SendMessageParams, SendMessageResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('send-message', context, subpath, commander);
  }

  public override getDefaultParams(): SendMessageParams {
    return new SendMessageParams({
      roomId: '',
      content: '',
      messageType: 'chat',
      deliveryOptions: {
        sendImmediately: true,
        typingIndicator: true,
        markdown: true,
        emojis: true
      }
    });
  }

  abstract execute(params: SendMessageParams): Promise<SendMessageResult>;
}