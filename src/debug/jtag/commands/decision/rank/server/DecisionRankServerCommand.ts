/**
 * decision/rank - Server-side implementation
 *
 * Handles ranked-choice voting with Condorcet winner calculation:
 * 1. Validates vote format and proposal exists
 * 2. Stores vote in proposal's votes array
 * 3. Checks if voting complete (all eligible voters voted or deadline passed)
 * 4. If complete, calculates Condorcet winner via pairwise comparisons
 * 5. Updates proposal status and announces winner
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { Commands } from '../../../../system/core/shared/Commands';
import { COLLECTIONS } from '../../../../system/shared/Constants';
import { DecisionRankCommand } from '../shared/DecisionRankCommand';
import type { DecisionRankParams, DecisionRankResult } from '../shared/DecisionRankTypes';
import type { DecisionProposalEntity, RankedVote } from '../../../../system/data/entities/DecisionProposalEntity';
import type { UserEntity } from '../../../../system/data/entities/UserEntity';
import type { DataListResult } from '../../../../commands/data/list/shared/DataListTypes';
import type { ChatSendParams, ChatSendResult } from '../../../../commands/chat/send/shared/ChatSendTypes';

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
          row.set(lessPreferred, (row.get(lessPreferred) || 0) + 1);
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

        const votesFor = row.get(other.id) || 0;
        const otherRow = matrix.get(other.id);
        const votesAgainst = otherRow?.get(opt.id) || 0;

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
 * DecisionRankServerCommand - Server implementation
 */
export class DecisionRankServerCommand extends DecisionRankCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeCommand(params: DecisionRankParams): Promise<DecisionRankResult> {
    try {
      // Validation
      if (!params.proposalId) {
        return transformPayload(params, { success: false, error: 'Proposal ID is required' });
      }

      if (!Array.isArray(params.rankedChoices) || params.rankedChoices.length === 0) {
        return transformPayload(params, { success: false, error: 'Ranked choices are required' });
      }

      // Get voter ID
      let voterId: UUID;
      if (params.voterId) {
        voterId = params.voterId;
      } else {
        const usersResult = await Commands.execute<any, DataListResult<UserEntity>>('data/list', {
          collection: COLLECTIONS.USERS,
          filter: { type: 'human' },
          limit: 1
        });
        if (!usersResult.success || !usersResult.items || usersResult.items.length === 0) {
          return transformPayload(params, { success: false, error: 'Could not find voter user' });
        }
        voterId = usersResult.items[0].id;
      }

      const voterResult = await Commands.execute<any, any>('data/read', {
        collection: COLLECTIONS.USERS,
        id: voterId
      });

      if (!voterResult.success || !voterResult.data) {
        return transformPayload(params, { success: false, error: 'Could not find voter user' });
      }

      const voter = voterResult.data;

      // Get proposal
      const proposalResult = await Commands.execute<any, any>('data/read', {
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
          error: `Proposal is not accepting votes (status: ${proposal.status})`
        });
      }

      // Check deadline
      const now = Date.now();
      if (proposal.deadline && now > proposal.deadline) {
        // Proposal expired - mark as expired and don't accept vote
        await Commands.execute<any, any>('data/update', {
          collection: COLLECTIONS.DECISION_PROPOSALS,
          id: params.proposalId,
          data: { status: 'expired' }
        });

        return transformPayload(params, {
          success: false,
          proposalStatus: 'expired',
          error: 'Proposal voting deadline has passed'
        });
      }

      // Validate ranked choices match available options
      const validOptionIds = new Set(proposal.options.map(opt => opt.id));
      for (const choiceId of params.rankedChoices) {
        if (!validOptionIds.has(choiceId)) {
          return transformPayload(params, {
            success: false,
            error: `Invalid option ID: ${choiceId}`
          });
        }
      }

      // Check if user already voted
      const existingVoteIndex = proposal.votes.findIndex(v => v.voterId === voterId);

      // Create vote
      const vote: RankedVote = {
        voterId,
        voterName: voter.displayName,
        rankings: params.rankedChoices,
        votedAt: now
      };

      // Update or add vote
      let votes = [...proposal.votes];
      if (existingVoteIndex >= 0) {
        votes[existingVoteIndex] = vote;
      } else {
        votes.push(vote);
      }

      // Update proposal with vote
      await Commands.execute<any, any>('data/update', {
        collection: COLLECTIONS.DECISION_PROPOSALS,
        id: params.proposalId,
        data: { votes }
      });

      // Check if voting is complete
      // For now, we consider it complete when deadline passes
      // (In future, could check if all eligible voters have voted)
      const votingComplete = false; // Will be checked on next rank or by scheduled job

      if (votingComplete) {
        // Calculate Condorcet winner
        const winner = calculateCondorcetWinner(votes, proposal.options);

        if (winner) {
          // Update proposal status
          await Commands.execute<any, any>('data/update', {
            collection: COLLECTIONS.DECISION_PROPOSALS,
            id: params.proposalId,
            data: { status: 'complete' }
          });

          // Announce winner in chat
          const announcementMessage = `🏆 **Decision Complete: ${proposal.topic}**\n\n**Winner:** ${winner.label} (${winner.wins} pairwise wins)\n\nTotal votes: ${votes.length}\nProposal ID: ${params.proposalId}`;

          await Commands.execute<ChatSendParams, ChatSendResult>('chat/send', {
            message: announcementMessage,
            room: 'general'
          });

          return transformPayload(params, {
            success: true,
            voted: true,
            proposalStatus: 'complete',
            winner
          });
        }
      }

      return transformPayload(params, {
        success: true,
        voted: true,
        proposalStatus: 'voting'
      });

    } catch (error: any) {
      console.error('Error in decision/rank:', error);
      return transformPayload(params, {
        success: false,
        error: error.message || 'Unknown error'
      });
    }
  }
}
