import { CommandBase } from '@commandBase';
import type { ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { UUID } from 'crypto';
import { RoomEventSubscriptionParams } from './RoomEventTypes';
import type { RoomEventSubscriptionResult } from './RoomEventTypes';

export abstract class RoomEventCommand extends CommandBase<RoomEventSubscriptionParams, RoomEventSubscriptionResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('room-events', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): RoomEventSubscriptionParams {
    return new RoomEventSubscriptionParams({
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
    }, this.context, sessionId);
  }

  abstract execute(params: RoomEventSubscriptionParams): Promise<RoomEventSubscriptionResult>;
}