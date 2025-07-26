import {CommandBase, type ICommandDaemon} from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { SendRoomEventParams, type SendRoomEventResult } from './SendRoomEventTypes';

export abstract class SendRoomEventCommand extends CommandBase<SendRoomEventParams, SendRoomEventResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('send-room-event', context, subpath, commander);
  }

  public override getDefaultParams(): SendRoomEventParams {
    return new SendRoomEventParams({
      roomId: '',
      sourceParticipantId: '',
      sourceParticipantType: 'human',
      eventType: 'custom_event',
      eventData: {},
      priority: 'normal',
      deliveryOptions: {
        guaranteedDelivery: true,
        deliverToAll: true,
        immediateDelivery: true,
        batchWithOthers: false
      }
    });
  }

  abstract execute(params: SendRoomEventParams): Promise<SendRoomEventResult>;
}