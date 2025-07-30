import {CommandBase, type ICommandDaemon} from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import type { UUID } from '@shared/CrossPlatformUUID';
import { type SendRoomEventParams, type SendRoomEventResult, createSendRoomEventParams } from '@chatSendRoomEvent/shared/SendRoomEventTypes';

export abstract class SendRoomEventCommand extends CommandBase<SendRoomEventParams, SendRoomEventResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('send-room-event', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): SendRoomEventParams {
    return createSendRoomEventParams(this.context, sessionId, {
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