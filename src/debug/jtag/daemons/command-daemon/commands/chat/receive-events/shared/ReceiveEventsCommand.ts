import { ChatCommandBase } from '@chatShared/ChatCommandBase';
import type { ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { UUID } from 'crypto';
import { ReceiveEventsParams, type ReceiveEventsResult } from './ReceiveEventsTypes';

export abstract class ReceiveEventsCommand extends ChatCommandBase<ReceiveEventsParams, ReceiveEventsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('receive-events', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): ReceiveEventsParams {
    return new ReceiveEventsParams({
      roomId: '',
      eventTypes: ['message', 'room_event'],
      maxEvents: 100,
      timeoutMs: 30000
    }, this.context, sessionId);
  }

  abstract execute(params: ReceiveEventsParams): Promise<ReceiveEventsResult>;
}