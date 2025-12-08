/**
 * Decision Vote Command - Server Implementation
 *
 * Cast ranked-choice vote on a proposal
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { DecisionVoteParams, DecisionVoteResult } from '../shared/DecisionVoteTypes';
import { createDecisionVoteResultFromParams } from '../shared/DecisionVoteTypes';
import { DecisionEntity } from '../../../../system/data/entities/DecisionEntity';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';
import type { DataListParams, DataListResult } from '../../../data/list/shared/DataListTypes';
import type { DataUpdateParams, DataUpdateResult } from '../../../data/update/shared/DataUpdateTypes';
import { UserIdentityResolver } from '../../../../system/user/shared/UserIdentityResolver';
import { UserEntity } from '../../../../system/data/entities/UserEntity';

export class DecisionVoteServerCommand extends CommandBase<DecisionVoteParams, DecisionVoteResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('decision/vote', context, subpath, commander);
  }

  async execute(params: DecisionVoteParams): Promise<DecisionVoteResult> {
    // 1. Get voter identity (auto-detect caller)
    const voter = await this.findCallerIdentity(params);

    // 2. Validate params
    if (!params.proposalId || params.proposalId.trim().length === 0) {
      throw new Error('Proposal ID is required');
    }
    if (!params.rankedChoices || params.rankedChoices.length === 0) {
      throw new Error('At least one ranked choice is required');
    }

    // 3. Find the proposal
    const proposal = await this.findProposal(params);

    // 4. Validate proposal state
    if (proposal.status !== 'open') {
      throw new Error(`Proposal ${params.proposalId} is ${proposal.status}, not accepting votes`);
    }

    // Check voting deadline
    if (proposal.votingDeadline && new Date() > proposal.votingDeadline) {
      throw new Error(`Voting deadline has passed for proposal ${params.proposalId}`);
    }

    // 5. Validate ranked choices
    this.validateRankedChoices(params.rankedChoices, proposal);

    // 6. Check if user already voted
    const existingVoteIndex = proposal.votes.findIndex(v => v.voterId === voter.id);
    if (existingVoteIndex >= 0) {
      // User is changing their vote
      proposal.votes[existingVoteIndex] = {
        voterId: voter.id,
        voterName: voter.entity.displayName,
        rankedChoices: params.rankedChoices,
        timestamp: new Date().toISOString(),
        comment: params.comment
      };

      // Add audit log entry (ensure auditLog exists)
      if (!proposal.auditLog) {
        proposal.auditLog = [];
      }
      proposal.auditLog.push({
        timestamp: new Date().toISOString(),
        userId: voter.id,
        action: 'vote_changed',
        details: {
          rankedChoices: params.rankedChoices,
          comment: params.comment
        }
      });

      console.log(`🔄 Vote changed by ${voter.entity.displayName} on proposal ${params.proposalId}`);
    } else {
      // New vote
      proposal.votes.push({
        voterId: voter.id,
        voterName: voter.entity.displayName,
        rankedChoices: params.rankedChoices,
        timestamp: new Date().toISOString(),
        comment: params.comment
      });

      // Add audit log entry (ensure auditLog exists)
      if (!proposal.auditLog) {
        proposal.auditLog = [];
      }
      proposal.auditLog.push({
        timestamp: new Date().toISOString(),
        userId: voter.id,
        action: 'vote_cast',
        details: {
          rankedChoices: params.rankedChoices,
          comment: params.comment
        }
      });

      console.log(`✅ Vote cast by ${voter.entity.displayName} on proposal ${params.proposalId}`);
    }

    // 7. Update proposal in database
    const updateResult = await Commands.execute<DataUpdateParams<DecisionEntity>, DataUpdateResult<DecisionEntity>>(
      'data/update',
      {
        collection: DecisionEntity.collection,
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
      rankedChoices: params.rankedChoices,
      votedAt: new Date().toISOString(),
      voteCount: proposal.votes.length
    });
  }

  /**
   * Find proposal by proposalId
   */
  private async findProposal(params: DecisionVoteParams): Promise<DecisionEntity> {
    const result = await Commands.execute<DataListParams<DecisionEntity>, DataListResult<DecisionEntity>>(
      'data/list',
      {
        collection: DecisionEntity.collection,
        filter: { proposalId: params.proposalId },
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
  private validateRankedChoices(rankedChoices: string[], proposal: DecisionEntity): void {
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
   * Find caller identity using UserIdentityResolver
   * Auto-detects Claude Code, Joel (human), etc. based on process info
   */
  private async findCallerIdentity(params: DecisionVoteParams): Promise<{ id: UUID; entity: UserEntity }> {
    // Use UserIdentityResolver to detect calling process
    const identity = await UserIdentityResolver.resolve();

    // If user exists in database, return it
    if (identity.exists && identity.userId) {
      const result = await Commands.execute<DataListParams<UserEntity>, DataListResult<UserEntity>>(
        'data/list',
        {
          collection: UserEntity.collection,
          filter: { id: identity.userId },
          limit: 1,
          context: params.context,
          sessionId: params.sessionId
        }
      );

      if (result.success && result.items && result.items.length > 0) {
        const user = result.items[0];
        return { id: user.id, entity: user };
      }
    }

    // User doesn't exist - throw error with helpful message
    throw new Error(
      `Detected caller: ${identity.displayName} (${identity.uniqueId}) but user not found in database. ` +
      `Run seed script to create users.`
    );
  }
}
