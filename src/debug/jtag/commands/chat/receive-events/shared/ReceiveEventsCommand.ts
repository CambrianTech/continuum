import { ChatCommandBase } from '@commandsChat/shared/ChatCommandBase';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { type ReceiveEventsParams, type ReceiveEventsResult, createReceiveEventsParams } from '@chatReceiveEvents/shared/ReceiveEventsTypes';

export abstract class ReceiveEventsCommand extends ChatCommandBase<ReceiveEventsParams, ReceiveEventsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('receive-events', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): ReceiveEventsParams {
    return createReceiveEventsParams(this.context, sessionId, {
      roomId: '',
      eventTypes: ['message', 'room_event'],
      maxEvents: 100,
      timeoutMs: 30000
    });
  }

  abstract execute(params: ReceiveEventsParams): Promise<ReceiveEventsResult>;
}