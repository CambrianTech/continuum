/**
 * Decision View Command - Server Implementation
 *
 * View detailed information about a specific governance proposal
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { toShortId } from '@system/core/types/CrossPlatformUUID';
import type { DecisionViewParams, DecisionViewResult } from '../shared/DecisionViewTypes';
import { createDecisionViewResultFromParams } from '../shared/DecisionViewTypes';
import type { DataListParams, DataListResult } from '@commands/data/list/shared/DataListTypes';
import type { DataReadParams, DataReadResult } from '@commands/data/read/shared/DataReadTypes';
import type { DecisionProposalEntity } from '@system/data/entities/DecisionProposalEntity';

export class DecisionViewServerCommand extends CommandBase<DecisionViewParams, DecisionViewResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Decision View', context, subpath, commander);
  }

  async execute(params: DecisionViewParams): Promise<DecisionViewResult> {
    console.log('🔧 SERVER: Executing Decision View', params);

    // Validate proposalId parameter
    if (!params.proposalId || params.proposalId.trim() === '') {
      const errorMsg = 'Missing required parameter: proposalId';
      return createDecisionViewResultFromParams(params, {
        success: false,
        proposal: null,
        summary: errorMsg,
        error: errorMsg as any  // ToolRegistry stringifyError handles strings
      });
    }

    try {
      // Query proposal from database using Commands pattern
      const { Commands } = await import('@system/core/shared/Commands');
      const { COLLECTIONS } = await import('@system/shared/Constants');

      // Resolve short IDs to full UUIDs using CrossPlatformUUID utilities
      const { isShortId, normalizeShortId } = await import('@system/core/types/CrossPlatformUUID');
      let resolvedProposalId = params.proposalId;

      // Check if proposalId is a short ID (6 hex chars, optionally prefixed with #)
      if (isShortId(params.proposalId)) {
        const proposalShortId = normalizeShortId(params.proposalId);

        // Query for proposals ending with this short ID
        const proposalsResult = await Commands.execute<DataListParams, DataListResult<DecisionProposalEntity>>(DATA_COMMANDS.LIST, {
          collection: COLLECTIONS.DECISION_PROPOSALS,
          limit: 100
        });

        if (proposalsResult.success && proposalsResult.items) {
          const matching = proposalsResult.items.find((p: any) => p.id.endsWith(proposalShortId));
          if (matching) {
            resolvedProposalId = matching.id;
            console.log(`Resolved short ID #${proposalShortId} to full UUID ${resolvedProposalId}`);
          }
        }
      }

      const proposalResult = await Commands.execute<DataReadParams, DataReadResult<DecisionProposalEntity>>(DATA_COMMANDS.READ, {
        collection: COLLECTIONS.DECISION_PROPOSALS,
        id: resolvedProposalId
      });

      if (!proposalResult.success || !proposalResult.data) {
        const errorMsg = `Proposal not found with ID: ${params.proposalId}`;
        return createDecisionViewResultFromParams(params, {
          success: false,
          proposal: null,
          summary: errorMsg,
          error: errorMsg as any  // ToolRegistry stringifyError handles strings
        });
      }

      const proposal = proposalResult.data;

      // Generate human-readable summary
      const voteCount = proposal.votes?.length || 0;
      const optionCount = proposal.options?.length || 0;
      const timeRemaining = proposal.deadline ? Math.max(0, proposal.deadline - Date.now()) : 0;
      const hoursRemaining = Math.floor(timeRemaining / (1000 * 60 * 60));
      const proposalShortId = proposal.id ? toShortId(proposal.id) : 'unknown';

      let summary = `**${proposal.topic || 'Untitled'}** (#${proposalShortId})\n\n`;
      summary += `Status: ${proposal.status || 'unknown'}\n`;
      summary += `Options: ${optionCount}\n`;
      summary += `Votes: ${voteCount}\n`;

      if (proposal.status === 'voting' && hoursRemaining > 0) {
        summary += `Time remaining: ${hoursRemaining}h\n`;
      }

      summary += `\n**Options (use these IDs for voting):**\n`;
      for (const option of proposal.options || []) {
        const optionId = option.id || 'unknown';
        summary += `- ${optionId} → ${option.label || 'Option'}: ${option.description || 'No description'}\n`;
      }

      return createDecisionViewResultFromParams(params, {
        success: true,
        proposal: proposal as any,  // Type mismatch between DecisionEntity and DecisionProposalEntity
        summary
      });

    } catch (error: any) {
      console.error('Error in decision/view:', error);
      const errorMsg = `Error retrieving proposal: ${error.message || 'Unknown error'}`;
      return createDecisionViewResultFromParams(params, {
        success: false,
        proposal: null,
        summary: errorMsg,
        error: errorMsg as any  // ToolRegistry stringifyError handles strings
      });
    }
  }
}
