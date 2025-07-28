import { ReceiveEventsCommand } from '@chatReceiveEvents/shared/ReceiveEventsCommand';
import type { JTAGContext } from '@shared/JTAGTypes';
import type { ICommandDaemon } from '@commandBase';
import { type ReceiveEventsParams, type ReceiveEventsResult, createReceiveEventsResult, type ChatEvent } from '@chatReceiveEvents/shared/ReceiveEventsTypes';

export class ReceiveEventsServerCommand extends ReceiveEventsCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  async execute(params: ReceiveEventsParams): Promise<ReceiveEventsResult> {
    this.validateChatParams(params);
    this.logChatOperation('Receiving events', params.roomId, `types: ${params.eventTypes?.join(', ')}`);

    try {
      // Simulate event stream listening (simplified)
      const mockEvents: ChatEvent[] = [
        {
          id: 'event_1',
          type: 'message',
          data: { content: 'Hello from room' },
          timestamp: new Date().toISOString(),
          roomId: params.roomId,
          senderId: 'user_123'
        }
      ];

      return createReceiveEventsResult(params.context, params.sessionId, {
        roomId: params.roomId,
        success: true,
        timestamp: new Date().toISOString(),
        events: mockEvents,
        eventCount: mockEvents.length,
        streamActive: true
      });

    } catch (error: any) {
      return this.createChatErrorResult(params.sessionId, params.roomId, `Event receiving failed: ${error.message}`) as ReceiveEventsResult;
    }
  }
}