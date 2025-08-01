import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { GetChatHistoryParams } from './GetChatHistoryTypes';
import type { GetChatHistoryResult } from './GetChatHistoryTypes';

export abstract class GetChatHistoryCommand extends CommandBase<GetChatHistoryParams, GetChatHistoryResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('get-chat-history', context, subpath, commander);
  }

  public override getDefaultParams(): GetChatHistoryParams {
    return new GetChatHistoryParams({
      personaId: '',
      historyScope: {
        scopeType: 'active_rooms',
        maxMessagesPerRoom: 50,
        maxTotalMessages: 200,
        relevanceThreshold: 0.3,
        includePersonaMessages: true,
        includeSystemMessages: false,
        participantFilter: {
          includeHumans: true,
          includePersonas: true,
          includeRagAIs: true,
          activeParticipantsOnly: false
        },
        contentTypeFilter: {
          includeTextOnly: false,
          includeMediaMessages: true,
          includeLinks: true,
          minLength: 10,
          complexityLevel: 'any'
        }
      },
      timeRange: {
        lastHours: 6,
        currentSessionOnly: false
      },
      contentFilters: {
        messageTypes: ['chat', 'educational_response', 'question', 'knowledge_share'],
        learningContent: true,
        teachingMoments: true,
        questions: true,
        academyContent: true,
        capabilityDemonstrations: true
      },
      contextOptimization: {
        tokenOptimization: true,
        maxTokens: 8000,
        compressionEnabled: true,
        summarizationEnabled: true,
        relevanceRanking: true,
        contextualGrouping: true,
        structuredFormat: true,
        personalizedRelevance: true,
        adaptiveCompression: true
      },
      multiContextIntegration: true
    });
  }

  abstract execute(params: GetChatHistoryParams): Promise<GetChatHistoryResult>;
}