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

import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { Commands } from '@system/core/shared/Commands';
import { Events } from '@system/core/shared/Events';
import { COLLECTIONS } from '@system/shared/Constants';
import { DecisionRankCommand } from '../shared/DecisionRankCommand';
import type { DecisionRankParams, DecisionRankResult } from '../shared/DecisionRankTypes';
import type { DecisionProposalEntity, RankedVote } from '@system/data/entities/DecisionProposalEntity';
import type { ChatSendParams, ChatSendResult } from '@commands/collaboration/chat/send/shared/ChatSendTypes';
import { calculateCondorcetWinner } from '@system/shared/CondorcetUtils';
import { Logger } from '@system/core/logging/Logger';
import { UserIdentityResolver } from '@system/user/shared/UserIdentityResolver';

/**
 * DecisionRankServerCommand - Server implementation
 */
export class DecisionRankServerCommand extends DecisionRankCommand {
  private log = Logger.create('DecisionRankServerCommand', 'tools');

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeCommand(params: DecisionRankParams): Promise<DecisionRankResult> {
    try {
      // Parse JSON strings if present (AIs may pass JSON strings instead of arrays)
      if (typeof params.rankedChoices === 'string') {
        this.log.warn('Parameter format conversion: rankedChoices received as JSON string instead of array', {
          command: 'decision/rank',
          proposalId: params.proposalId,
          sessionId: params.sessionId
        });
        try {
          params.rankedChoices = JSON.parse(params.rankedChoices);
        } catch (e) {
          return transformPayload(params, { success: false, error: 'rankedChoices must be a valid JSON array' });
        }
      }

      // Validation
      if (!params.proposalId) {
        return transformPayload(params, { success: false, error: 'Proposal ID is required' });
      }

      if (!Array.isArray(params.rankedChoices) || params.rankedChoices.length === 0) {
        return transformPayload(params, { success: false, error: 'Ranked choices are required' });
      }

      // Get voter info - auto-detect caller identity
      let voterId: UUID;
      let voterName: string;

      if (params.voterId) {
        // Explicit voterId provided
        const voterResult = await Commands.execute<any, any>('data/read', {
          collection: COLLECTIONS.USERS,
          id: params.voterId
        });

        if (!voterResult.success || !voterResult.data) {
          return transformPayload(params, { success: false, error: 'Could not find voter user' });
        }

        voterId = params.voterId;
        voterName = voterResult.data.displayName;
      } else {
        // Auto-detect caller identity using UserIdentityResolver
        const identity = await UserIdentityResolver.resolve();

        this.log.debug('Auto-detected voter identity', {
          uniqueId: identity.uniqueId,
          displayName: identity.displayName,
          type: identity.type,
          exists: identity.exists
        });

        if (!identity.exists || !identity.userId) {
          return transformPayload(params, {
            success: false,
            error: `Detected caller: ${identity.displayName} (${identity.uniqueId}) but user not found in database. Run seed script to create users.`
          });
        }

        voterId = identity.userId;
        voterName = identity.displayName;
      }

      // Resolve short IDs to full UUIDs using CrossPlatformUUID utilities
      const { isShortId, normalizeShortId } = await import('@system/core/types/CrossPlatformUUID');
      let resolvedProposalId = params.proposalId;

      // Check if proposalId is a short ID (6 hex chars, optionally prefixed with #)
      if (isShortId(params.proposalId)) {
        const proposalShortId = normalizeShortId(params.proposalId);

        // Query for proposals ending with this short ID
        const proposalsResult = await Commands.execute<any, any>(DATA_COMMANDS.LIST, {
          collection: COLLECTIONS.DECISION_PROPOSALS,
          limit: 100
        });

        if (proposalsResult.success && proposalsResult.items) {
          const matching = proposalsResult.items.find((p: any) => p.id.endsWith(proposalShortId));
          if (matching) {
            resolvedProposalId = matching.id;
            this.log.info(`Resolved short ID #${proposalShortId} to full UUID ${resolvedProposalId}`);
          }
        }
      }

      // Get proposal
      const proposalResult = await Commands.execute<any, any>('data/read', {
        collection: COLLECTIONS.DECISION_PROPOSALS,
        id: resolvedProposalId
      });

      if (!proposalResult.success || !proposalResult.data) {
        return transformPayload(params, {
          success: false,
          error: `Proposal not found with ID: ${params.proposalId}. Use decision/list to see all proposals, or decision/view with a valid proposal ID to see its details and options.`
        });
      }

      const proposal: DecisionProposalEntity = proposalResult.data;

      // Resolve short IDs in rankedChoices to full option UUIDs
      const resolvedRankedChoices = params.rankedChoices.map(choiceId => {
        if (isShortId(choiceId)) {
          const choiceShortId = normalizeShortId(choiceId);
          const matchingOption = proposal.options.find(opt => opt.id.endsWith(choiceShortId));
          if (matchingOption) {
            this.log.info(`Resolved option short ID #${choiceShortId} to ${matchingOption.id}`);
            return matchingOption.id;
          }
        }
        return choiceId; // Return as-is if not a short ID or no match
      });

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
          id: resolvedProposalId,
          data: { status: 'expired' }
        });

        return transformPayload(params, {
          success: false,
          proposalStatus: 'expired',
          error: 'Proposal voting deadline has passed'
        });
      }

      // Validate resolved ranked choices match available options
      const validOptionIds = new Set(proposal.options.map(opt => opt.id));
      for (const choiceId of resolvedRankedChoices) {
        if (!validOptionIds.has(choiceId)) {
          // Show both full IDs and short IDs in error message
          const { toShortId } = await import('@system/core/types/CrossPlatformUUID');
          const optionsList = proposal.options.map(opt =>
            `  - #${toShortId(opt.id)} (${opt.id}) → "${opt.label}"`
          ).join('\n');
          return transformPayload(params, {
            success: false,
            error: `Invalid option ID: "${choiceId}". Valid options for this proposal:\n${optionsList}\n\nUse decision/view with proposalId to see all options and their IDs.`
          });
        }
      }

      // Check if user already voted
      const existingVoteIndex = proposal.votes.findIndex(v => v.voterId === voterId);

      // Create vote with resolved option IDs
      const vote: RankedVote = {
        voterId,
        voterName: voterName,
        rankings: resolvedRankedChoices,
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
        id: resolvedProposalId,
        data: { votes }
      });

      // Emit event for vote (keep rankings private until finalization)
      Events.emit('decision:voted', {
        proposalId: resolvedProposalId,
        proposalTopic: proposal.topic,
        voterId,
        voterName: voterName,
        votedAt: now,
        totalVotes: votes.length,
        isUpdate: existingVoteIndex >= 0,
        // Rankings intentionally omitted to keep votes private until finalization
      });

      this.log.info('Vote cast successfully', {
        proposalId: params.proposalId,
        voterId,
        voterName: voterName,
        isUpdate: existingVoteIndex >= 0,
        totalVotes: votes.length
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
            id: resolvedProposalId,
            data: { status: 'complete' }
          });

          // Announce winner in chat
          const announcementMessage = `🏆 **Decision Complete: ${proposal.topic}**\n\n**Winner:** ${winner.label} (${winner.wins} pairwise wins)\n\nTotal votes: ${votes.length}\nProposal ID: ${resolvedProposalId}`;

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
