import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { type RoomEventSubscriptionParams, createRoomEventSubscriptionParams } from '@chatRoomEvents/shared/RoomEventTypes';
import type { RoomEventSubscriptionResult } from '@chatRoomEvents/shared/RoomEventTypes';

export abstract class RoomEventCommand extends CommandBase<RoomEventSubscriptionParams, RoomEventSubscriptionResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('room-events', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): RoomEventSubscriptionParams {
    return createRoomEventSubscriptionParams(this.context, sessionId, {
      participantId: '',
      participantType: 'human',
      roomId: '',
      eventTypes: [
        'message_sent',
        'participant_joined',
        'participant_left',
        'typing_started',
        'typing_stopped',
        'widget_activated',
        'widget_state_changed',
        'academy_phase_changed',
        'capability_demonstrated'
      ],
      eventFilters: {
        priorityThreshold: 'normal',
        rateLimitPerSecond: 10,
        batchingEnabled: true,
        relevanceThreshold: 0.3
      },
      subscriptionOptions: {
        deliveryMode: 'real_time',
        guaranteedDelivery: true,
        duplicateFiltering: true,
        orderPreservation: true,
        bufferSize: 100,
        autoUnsubscribeOnLeave: true,
        backfillOnSubscribe: true,
        backfillCount: 20
      }
    });
  }

  abstract execute(params: RoomEventSubscriptionParams): Promise<RoomEventSubscriptionResult>;
}