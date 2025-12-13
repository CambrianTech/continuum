/**
 * Decision View Command - Server Implementation
 *
 * View detailed information about a specific governance proposal
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
// import { ValidationError } from '../../../../system/core/types/ErrorTypes';  // Uncomment when adding validation
import type { DecisionViewParams, DecisionViewResult } from '../shared/DecisionViewTypes';
import { createDecisionViewResultFromParams } from '../shared/DecisionViewTypes';

export class DecisionViewServerCommand extends CommandBase<DecisionViewParams, DecisionViewResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Decision View', context, subpath, commander);
  }

  async execute(params: DecisionViewParams): Promise<DecisionViewResult> {
    console.log('🔧 SERVER: Executing Decision View', params);

    // Validate proposalId parameter
    if (!params.proposalId || params.proposalId.trim() === '') {
      return createDecisionViewResultFromParams(params, {
        success: false,
        proposal: null,
        summary: 'Missing required parameter: proposalId'
      });
    }

    try {
      // Query proposal from database using Commands pattern
      const { Commands } = await import('../../../../system/core/shared/Commands');
      const { COLLECTIONS } = await import('../../../../system/shared/Constants');

      const proposalResult = await Commands.execute<any, any>('data/read', {
        collection: COLLECTIONS.DECISION_PROPOSALS,
        id: params.proposalId
      });

      if (!proposalResult.success || !proposalResult.data) {
        return createDecisionViewResultFromParams(params, {
          success: false,
          proposal: null,
          summary: `Proposal not found with ID: ${params.proposalId}`
        });
      }

      const proposal = proposalResult.data;

      // Generate human-readable summary
      const voteCount = proposal.votes?.length || 0;
      const optionCount = proposal.options?.length || 0;
      const timeRemaining = proposal.deadline ? Math.max(0, proposal.deadline - Date.now()) : 0;
      const hoursRemaining = Math.floor(timeRemaining / (1000 * 60 * 60));

      let summary = `**${proposal.topic}**\n\n`;
      summary += `Status: ${proposal.status}\n`;
      summary += `Options: ${optionCount}\n`;
      summary += `Votes: ${voteCount}\n`;

      if (proposal.status === 'voting' && hoursRemaining > 0) {
        summary += `Time remaining: ${hoursRemaining}h\n`;
      }

      summary += `\n**Options (use these IDs for voting):**\n`;
      for (const option of proposal.options || []) {
        summary += `- ID: \`${option.id}\` - ${option.label}: ${option.description}\n`;
      }

      return createDecisionViewResultFromParams(params, {
        success: true,
        proposal: proposal as any,  // Type mismatch between DecisionEntity and DecisionProposalEntity
        summary
      });

    } catch (error: any) {
      console.error('Error in decision/view:', error);
      return createDecisionViewResultFromParams(params, {
        success: false,
        proposal: null,
        summary: `Error retrieving proposal: ${error.message || 'Unknown error'}`
      });
    }
  }
}
