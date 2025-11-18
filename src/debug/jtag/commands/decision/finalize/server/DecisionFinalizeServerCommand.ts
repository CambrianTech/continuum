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
import type { DecisionProposalEntity, RankedVote } from '../../../../system/data/entities/DecisionProposalEntity';
import type { ChatSendParams, ChatSendResult } from '../../../../commands/chat/send/shared/ChatSendTypes';
import type { DataReadParams, DataReadResult } from '../../../../commands/data/read/shared/DataReadTypes';
import type { DataUpdateParams, DataUpdateResult } from '../../../../commands/data/update/shared/DataUpdateTypes';

/**
 * Calculate Condorcet winner using pairwise comparisons
 * Returns the option that beats all others in head-to-head matchups
 * If no Condorcet winner exists (cycle), returns option with most pairwise wins
 */
function calculateCondorcetWinner(
  votes: RankedVote[],
  options: Array<{ id: string; label: string }>
): { optionId: string; label: string; wins: number } | null {
  if (votes.length === 0 || options.length === 0) {
    return null;
  }

  // Create pairwise comparison matrix
  const matrix = new Map<string, Map<string, number>>();

  // Initialize matrix
  for (const opt of options) {
    const innerMap = new Map<string, number>();
    for (const other of options) {
      if (opt.id !== other.id) {
        innerMap.set(other.id, 0);
      }
    }
    matrix.set(opt.id, innerMap);
  }

  // Count pairwise preferences
  for (const vote of votes) {
    const rankedChoices = vote.rankings;

    // Compare each pair of options in this voter's ranking
    for (let i = 0; i < rankedChoices.length; i++) {
      for (let j = i + 1; j < rankedChoices.length; j++) {
        const preferred = rankedChoices[i];
        const lessPreferred = rankedChoices[j];

        // Increment: preferred beats lessPreferred
        const row = matrix.get(preferred);
        if (row) {
          row.set(lessPreferred, (row.get(lessPreferred) ?? 0) + 1);
        }
      }
    }
  }

  // Find Condorcet winner (option that beats all others head-to-head)
  const wins = new Map<string, number>();

  for (const opt of options) {
    let winsCount = 0;
    const row = matrix.get(opt.id);

    if (row) {
      for (const other of options) {
        if (opt.id === other.id) continue;

        const votesFor = row.get(other.id) ?? 0;
        const otherRow = matrix.get(other.id);
        const votesAgainst = otherRow?.get(opt.id) ?? 0;

        // This option beats 'other' if it gets more votes in head-to-head
        if (votesFor > votesAgainst) {
          winsCount++;
        }
      }
    }

    wins.set(opt.id, winsCount);
  }

  // Find option with most pairwise wins
  let winner: { optionId: string; label: string; wins: number } | null = null;
  let maxWins = -1;

  for (const [optionId, winsCount] of wins.entries()) {
    if (winsCount > maxWins) {
      maxWins = winsCount;
      const option = options.find(o => o.id === optionId);
      if (option) {
        winner = { optionId, label: option.label, wins: winsCount };
      }
    }
  }

  return winner;
}

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
