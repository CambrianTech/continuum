/**
 * Decision Finalize Command - Server Implementation
 *
 * Close voting and calculate winner using Condorcet ranked-choice voting
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { DecisionFinalizeParams, DecisionFinalizeResult } from '../shared/DecisionFinalizeTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import { Events } from '../../../../system/core/shared/Events';
import { COLLECTIONS } from '../../../../system/shared/Constants';
import type { DecisionProposalEntity } from '../../../../system/data/entities/DecisionProposalEntity';
import type { UserEntity } from '../../../../system/data/entities/UserEntity';
import type { DataListResult } from '../../../../commands/data/list/shared/DataListTypes';
import type { ChatSendParams, ChatSendResult } from '../../../../commands/chat/send/shared/ChatSendTypes';
import { calculateCondorcetWinner } from '../../../../system/shared/CondorcetUtils';
import { Logger } from '../../../../system/core/logging/Logger';

export class DecisionFinalizeServerCommand extends CommandBase<DecisionFinalizeParams, DecisionFinalizeResult> {
  private log = Logger.create('DecisionFinalizeServerCommand', 'tools');

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Decision Finalize', context, subpath, commander);
  }

  async execute(params: DecisionFinalizeParams): Promise<DecisionFinalizeResult> {
    // Validate required parameters
    if (!params.proposalId) {
      throw new Error('Proposal ID is required');
    }

    // Get proposal
    const proposalResult = await Commands.execute<any, any>('data/read', {
      collection: COLLECTIONS.DECISION_PROPOSALS,
      id: params.proposalId
    });

    if (!proposalResult.success || !proposalResult.data) {
      throw new Error(`Proposal not found: ${params.proposalId}`);
    }

    const proposal: DecisionProposalEntity = proposalResult.data;

    // Check proposal status
    if (proposal.status === 'concluded') {
      throw new Error('Proposal already finalized');
    }

    if (proposal.status === 'cancelled') {
      throw new Error('Proposal was cancelled');
    }

    // Check if there are any votes
    if (!proposal.votes || proposal.votes.length === 0) {
      throw new Error('No votes cast - cannot finalize');
    }

    // Calculate Condorcet winner
    const winnerResult = calculateCondorcetWinner(proposal.votes, proposal.options);

    if (!winnerResult) {
      throw new Error('Could not determine winner (Condorcet paradox)');
    }

    // Get total eligible voters (all AIs for now)
    const usersResult = await Commands.execute<any, DataListResult<UserEntity>>('data/list', {
      collection: COLLECTIONS.USERS,
      filter: { type: { $in: ['agent', 'persona'] } },
      limit: 100
    });

    const totalEligible = usersResult.success && usersResult.items ? usersResult.items.length : 0;
    const totalVoted = proposal.votes.length;
    const percentage = totalEligible > 0 ? (totalVoted / totalEligible) * 100 : 0;

    // Calculate final tallies from Condorcet results
    // For now, just mark winner with their win count, others with 0
    const finalTallies: Record<string, number> = {};
    for (const option of proposal.options) {
      finalTallies[option.id] = option.id === winnerResult.optionId ? winnerResult.wins : 0;
    }

    // Update proposal status to concluded
    await Commands.execute<any, any>('data/update', {
      collection: COLLECTIONS.DECISION_PROPOSALS,
      id: params.proposalId,
      data: {
        status: 'concluded',
        results: {
          winningOption: winnerResult.optionId,
          rounds: [], // Condorcet doesn't have elimination rounds
          participationRate: percentage / 100,
          consensusStrength: winnerResult.wins / (proposal.options.length - 1) // Wins / total possible wins
        }
      }
    });

    this.log.info('Proposal finalized', {
      proposalId: params.proposalId,
      winner: winnerResult.label,
      totalVotes: totalVoted,
      participationRate: percentage
    });

    // Emit finalization event
    Events.emit('decision:finalized', {
      proposalId: params.proposalId,
      proposalTopic: proposal.topic,
      winnerId: winnerResult.optionId,
      winnerLabel: winnerResult.label,
      totalVotes: totalVoted,
      participationRate: percentage,
      consensusStrength: winnerResult.wins / (proposal.options.length - 1)
    });

    // Announce winner in chat
    const announcementMessage = `🏆 **Decision Finalized: ${proposal.topic}**

**Winner:** ${winnerResult.label}
**Confidence:** ${winnerResult.wins}/${proposal.options.length - 1} pairwise victories

**Participation:**
- Votes cast: ${totalVoted}
- Eligible voters: ${totalEligible}
- Turnout: ${percentage.toFixed(1)}%

Proposal ID: ${params.proposalId}`;

    await Commands.execute<ChatSendParams, ChatSendResult>('chat/send', {
      message: announcementMessage,
      room: 'general'
    });

    return transformPayload(params, {
      success: true,
      winner: winnerResult.optionId,
      rounds: [], // Condorcet doesn't use elimination rounds
      participation: {
        totalEligible,
        totalVoted,
        percentage
      },
      finalTallies
    });
  }
}
