import { ReceiveEventsCommand } from '../shared/ReceiveEventsCommand';
import type { JTAGContext } from '@shared/JTAGTypes';
import type { ICommandDaemon } from '@commandBase';
import { ReceiveEventsParams, ReceiveEventsResult, type ChatEvent } from '../shared/ReceiveEventsTypes';

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

      return new ReceiveEventsResult({
        roomId: params.roomId,
        success: true,
        environment: this.context.environment,
        timestamp: new Date().toISOString(),
        events: mockEvents,
        eventCount: mockEvents.length,
        streamActive: true
      });

    } catch (error: any) {
      return this.createChatErrorResult(params.roomId, `Event receiving failed: ${error.message}`) as ReceiveEventsResult;
    }
  }
}