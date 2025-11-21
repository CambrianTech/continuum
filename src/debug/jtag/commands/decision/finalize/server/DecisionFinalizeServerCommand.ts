/**
 * decision/finalize - Server-side implementation
 *
 * Manually finalizes a voting proposal:
 * 1. Validates proposal exists and is in voting state
 * 2. Calculates Condorcet winner from current votes
 * 3. Updates proposal status to complete
 * 4. Announces winner in chat
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { Commands } from '../../../../system/core/shared/Commands';
import { COLLECTIONS } from '../../../../system/shared/Constants';
import { DecisionFinalizeCommand } from '../shared/DecisionFinalizeCommand';
import type { DecisionFinalizeParams, DecisionFinalizeResult } from '../shared/DecisionFinalizeTypes';
import type { DecisionProposalEntity } from '../../../../system/data/entities/DecisionProposalEntity';
import type { ChatSendParams, ChatSendResult } from '../../../../commands/chat/send/shared/ChatSendTypes';
import type { DataReadParams, DataReadResult } from '../../../../commands/data/read/shared/DataReadTypes';
import type { DataUpdateParams, DataUpdateResult } from '../../../../commands/data/update/shared/DataUpdateTypes';
import { calculateCondorcetWinner } from '../../../../system/shared/CondorcetUtils';

/**
 * DecisionFinalizeServerCommand - Server implementation
 */
export class DecisionFinalizeServerCommand extends DecisionFinalizeCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeCommand(params: DecisionFinalizeParams): Promise<DecisionFinalizeResult> {
    try {
      // Validation
      if (!params.proposalId) {
        return transformPayload(params, { success: false, error: 'Proposal ID is required' });
      }

      // Get proposal
      const proposalResult = await Commands.execute<DataReadParams, DataReadResult<DecisionProposalEntity>>('data/read', {
        collection: COLLECTIONS.DECISION_PROPOSALS,
        id: params.proposalId
      });

      if (!proposalResult.success || !proposalResult.data) {
        return transformPayload(params, { success: false, error: 'Proposal not found' });
      }

      const proposal: DecisionProposalEntity = proposalResult.data;

      // Check proposal status
      if (proposal.status !== 'voting') {
        return transformPayload(params, {
          success: false,
          error: `Proposal cannot be finalized (status: ${proposal.status})`
        });
      }

      // Check if there are votes
      if (proposal.votes.length === 0) {
        return transformPayload(params, {
          success: false,
          error: 'Cannot finalize proposal with zero votes'
        });
      }

      // Calculate Condorcet winner
      const winner = calculateCondorcetWinner(proposal.votes, proposal.options);

      if (!winner) {
        return transformPayload(params, {
          success: false,
          error: 'Could not determine winner'
        });
      }

      // Update proposal status
      await Commands.execute<DataUpdateParams, DataUpdateResult>('data/update', {
        collection: COLLECTIONS.DECISION_PROPOSALS,
        id: params.proposalId,
        data: { status: 'complete' }
      });

      // Announce winner in chat
      const announcementMessage = `🏆 **Decision Complete: ${proposal.topic}**\n\n**Winner:** ${winner.label} (${winner.wins} pairwise wins)\n\nTotal votes: ${proposal.votes.length}\nProposal ID: ${params.proposalId}`;

      await Commands.execute<ChatSendParams, ChatSendResult>('chat/send', {
        message: announcementMessage,
        room: 'general'
      });

      return transformPayload(params, {
        success: true,
        proposalStatus: 'complete',
        winner,
        voteCount: proposal.votes.length
      });

    } catch (error: unknown) {
      console.error('Error in decision/finalize:', error);
      return transformPayload(params, {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
  }
}
