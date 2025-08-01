import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { ReceiveEventsParams } from './ReceiveEventsTypes';
import type { ReceiveEventsResult } from './ReceiveEventsTypes';

export abstract class ReceiveEventsCommand extends CommandBase<ReceiveEventsParams, ReceiveEventsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('receive-events', context, subpath, commander);
  }

  public override getDefaultParams(): ReceiveEventsParams {
    return new ReceiveEventsParams({
      personaId: '',
      eventTypes: [
        'message_received',
        'direct_mention', 
        'question_asked',
        'learning_opportunity',
        'help_requested'
      ],
      streamConfig: {
        streamType: 'websocket',
        heartbeatInterval: 30000,
        reconnectAttempts: 5,
        bufferSize: 100,
        batchingEnabled: true,
        compressionEnabled: true,
        guaranteedDelivery: true,
        duplicateDetection: true
      },
      eventFilters: {
        minimumPriority: 'low',
        relevanceThreshold: 0.3,
        maxEventsPerSecond: 10,
        burstLimit: 20,
        cooldownPeriod: 5000
      },
      responseConfig: {
        autoResponse: true,
        responseDelay: 1000,
        maxResponsesPerHour: 60,
        responseTimeWindow: 3600000
      },
      multiContextEnabled: true,
      contextRefreshInterval: 10000
    });
  }

  abstract execute(params: ReceiveEventsParams): Promise<ReceiveEventsResult>;
}