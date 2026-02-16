/**
 * Decision Vote Command - Server Implementation
 *
 * Cast ranked-choice vote on a proposal
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { DecisionVoteParams, DecisionVoteResult } from '../shared/DecisionVoteTypes';
import { createDecisionVoteResultFromParams } from '../shared/DecisionVoteTypes';

import type { DecisionProposalEntity } from '@system/data/entities/DecisionProposalEntity';
import { COLLECTIONS } from '@system/shared/Constants';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '@system/core/shared/Commands';
import type { DataListParams, DataListResult } from '@commands/data/list/shared/DataListTypes';
import type { DataUpdateParams, DataUpdateResult } from '@commands/data/update/shared/DataUpdateTypes';
import { UserEntity } from '@system/data/entities/UserEntity';

import { DataUpdate } from '../../../../data/update/shared/DataUpdateTypes';
import { DataList } from '../../../../data/list/shared/DataListTypes';
export class DecisionVoteServerCommand extends CommandBase<DecisionVoteParams, DecisionVoteResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/decision/vote', context, subpath, commander);
  }

  async execute(params: DecisionVoteParams): Promise<DecisionVoteResult> {
    // 1. Get voter from params.userId (auto-injected by infrastructure)
    const voter = await this.findUserById(params.userId, params);

    // 2. Parse rankedChoices if passed as JSON string (common from AI tool calls)
    let rankedChoices = params.rankedChoices;
    if (typeof rankedChoices === 'string') {
      try {
        rankedChoices = JSON.parse(rankedChoices);
      } catch {
        throw new Error('rankedChoices must be a valid JSON array of option IDs');
      }
    }
    if (!Array.isArray(rankedChoices)) {
      throw new Error('rankedChoices must be an array of option IDs');
    }

    // 3. Validate params
    if (!params.proposalId || params.proposalId.trim().length === 0) {
      throw new Error('Proposal ID is required');
    }
    if (!rankedChoices || rankedChoices.length === 0) {
      throw new Error('At least one ranked choice is required');
    }

    // 3. Find the proposal
    const proposal = await this.findProposal(params);

    // 4. Validate proposal state
    if (proposal.status !== 'voting') {
      throw new Error(`Proposal ${params.proposalId} is ${proposal.status}, not accepting votes`);
    }

    // Check voting deadline (deadline is a unix timestamp number)
    if (proposal.deadline && Date.now() > proposal.deadline) {
      throw new Error(`Voting deadline has passed for proposal ${params.proposalId}`);
    }

    // 5. Validate ranked choices
    this.validateRankedChoices(rankedChoices, proposal);

    // 6. Check if user already voted (RankedVote interface)
    const existingVoteIndex = proposal.votes.findIndex(v => v.voterId === voter.id);
    if (existingVoteIndex >= 0) {
      // User is changing their vote
      proposal.votes[existingVoteIndex] = {
        voterId: voter.id,
        voterName: voter.entity.displayName,
        rankings: rankedChoices,
        votedAt: Date.now(),
        reasoning: params.comment
      };
      console.log(`🔄 Vote changed by ${voter.entity.displayName} on proposal ${params.proposalId}`);
    } else {
      // New vote
      proposal.votes.push({
        voterId: voter.id,
        voterName: voter.entity.displayName,
        rankings: rankedChoices,
        votedAt: Date.now(),
        reasoning: params.comment
      });
      console.log(`✅ Vote cast by ${voter.entity.displayName} on proposal ${params.proposalId}`);
    }

    // 7. Update proposal in database
    const updateResult = await DataUpdate.execute<DecisionProposalEntity>({
        collection: COLLECTIONS.DECISION_PROPOSALS,
        id: proposal.id,
        data: proposal,
        context: params.context,
        sessionId: params.sessionId
      }
    );

    if (!updateResult.success || !updateResult.data) {
      throw new Error(`Failed to update proposal: ${updateResult.error || 'Unknown error'}`);
    }

    return createDecisionVoteResultFromParams(params, {
      success: true,
      proposalId: params.proposalId,
      voterId: voter.id,
      voterName: voter.entity.displayName,
      rankedChoices: rankedChoices,
      votedAt: new Date().toISOString(),
      voteCount: proposal.votes.length
    });
  }

  /**
   * Find proposal by proposalId (which is the entity's `id` field)
   */
  private async findProposal(params: DecisionVoteParams): Promise<DecisionProposalEntity> {
    const result = await DataList.execute<DecisionProposalEntity>({
        collection: COLLECTIONS.DECISION_PROPOSALS,
        filter: { id: params.proposalId },
        limit: 1,
        context: params.context,
        sessionId: params.sessionId
      }
    );

    if (!result.success || !result.items || result.items.length === 0) {
      throw new Error(`Proposal not found: ${params.proposalId}`);
    }

    return result.items[0];
  }

  /**
   * Validate ranked choices against proposal options
   */
  private validateRankedChoices(rankedChoices: string[], proposal: DecisionProposalEntity): void {
    // Check each choice is a valid option ID
    const validOptionIds = proposal.options.map(opt => opt.id);
    for (const choiceId of rankedChoices) {
      if (!validOptionIds.includes(choiceId)) {
        throw new Error(`Invalid option ID: ${choiceId}`);
      }
    }

    // Check for duplicates
    const uniqueChoices = new Set(rankedChoices);
    if (uniqueChoices.size !== rankedChoices.length) {
      throw new Error('Ranked choices cannot contain duplicates');
    }

    // Voters must rank at least one option, but can rank all or some
    if (rankedChoices.length === 0) {
      throw new Error('Must rank at least one option');
    }
  }

  /**
   * Find user by ID from database
   */
  private async findUserById(userId: UUID, params: DecisionVoteParams): Promise<{ id: UUID; entity: UserEntity }> {
    const result = await DataList.execute<UserEntity>({
      collection: UserEntity.collection,
      filter: { id: userId },
      limit: 1,
      context: params.context,
      sessionId: params.sessionId
    });

    if (!result.success || !result.items || result.items.length === 0) {
      throw new Error(`User not found: ${userId}`);
    }

    const user = result.items[0];
    return { id: user.id, entity: user };
  }
}
