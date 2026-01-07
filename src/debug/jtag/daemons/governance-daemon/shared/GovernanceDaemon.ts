/**
 * Governance Daemon - Automated Governance Workflows
 *
 * Handles automated governance workflows:
 * - Auto-finalization of expired proposals
 * - Edge case detection (low participation, ties)
 * - Manual review flagging when needed
 */

import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { JTAGContext, JTAGMessage } from '../../../system/core/types/JTAGTypes';
import { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { Commands } from '../../../system/core/shared/Commands';
import { Events } from '../../../system/core/shared/Events';
import { COLLECTIONS } from '../../../system/shared/Constants';
import type { DecisionProposalEntity } from '../../../system/data/entities/DecisionProposalEntity';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import type { DecisionFinalizeParams, DecisionFinalizeResult } from '@commands/collaboration/decision/finalize/shared/DecisionFinalizeTypes';

/**
 * Governance Daemon - Universal governance workflow handler
 */
export abstract class GovernanceDaemon extends DaemonBase {
  public readonly subpath: string = 'governance';
  protected schedulerInterval: NodeJS.Timeout | null = null;
  protected isShuttingDown = false;

  // Configuration
  protected readonly CHECK_INTERVAL_MS = 30000; // 30 seconds (AI timescale)
  protected readonly MIN_PARTICIPATION_RATE = 0.3; // 30% minimum participation

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('governance-daemon', context, router);
  }

  /**
   * Initialize governance daemon and start scheduler
   */
  protected async initialize(): Promise<void> {
    this.log.info('Initializing governance daemon');

    // Start the auto-finalization scheduler
    this.startScheduler();

    this.log.info(`Governance daemon initialized - checking every ${this.CHECK_INTERVAL_MS}ms`);
  }

  /**
   * Handle incoming governance messages (currently unused - scheduler-based)
   */
  async handleMessage(message: JTAGMessage): Promise<any> {
    // Governance daemon is scheduler-based, not message-based
    // This method exists to satisfy DaemonBase contract
    return { success: false, error: 'Governance daemon does not handle messages' };
  }

  /**
   * Start the periodic scheduler to check for expired proposals
   */
  protected startScheduler(): void {
    this.schedulerInterval = setInterval(
      () => this.checkExpiredProposals(),
      this.CHECK_INTERVAL_MS
    );

    this.log.info('Auto-finalization scheduler started');
  }

  /**
   * Check for proposals past their deadline and finalize them
   */
  protected async checkExpiredProposals(): Promise<void> {
    if (this.isShuttingDown) {
      return;
    }

    try {
      const now = Date.now();

      // Query all active proposals past their deadline
      const result = await Commands.execute<DataListParams, DataListResult<DecisionProposalEntity>>(
        DATA_COMMANDS.LIST,
        {
          collection: COLLECTIONS.DECISION_PROPOSALS,
          filter: {
            status: 'voting',
            deadline: { $lte: now }
          },
          limit: 100
        }
      );

      if (!result.success || !result.items || result.items.length === 0) {
        // No expired proposals - this is the common case
        return;
      }

      this.log.info(`Found ${result.items.length} expired proposal(s) to finalize`);

      // Finalize each expired proposal
      for (const proposal of result.items) {
        await this.finalizeProposal(proposal);
      }

    } catch (error) {
      this.log.error('Error checking expired proposals', { error });
      // Don't throw - keep scheduler running
    }
  }

  /**
   * Finalize a single proposal with edge case handling
   */
  protected async finalizeProposal(proposal: DecisionProposalEntity): Promise<void> {
    try {
      this.log.info(`Finalizing proposal: ${proposal.topic}`, {
        proposalId: proposal.id,
        votes: proposal.votes.length,
        deadline: new Date(proposal.deadline).toISOString()
      });

      // Check for low participation (based on governance vote: flag for manual review)
      const hasLowParticipation = await this.checkLowParticipation(proposal);

      if (hasLowParticipation) {
        this.log.warn('Low participation detected - flagging for manual review', {
          proposalId: proposal.id,
          voteCount: proposal.votes.length
        });

        // Flag for manual review
        await this.flagForManualReview(proposal, 'low_participation');
        return;
      }

      // Attempt to finalize the proposal
      const result = await Commands.execute<DecisionFinalizeParams, DecisionFinalizeResult>(
        'decision/finalize',
        {
          proposalId: proposal.id
        }
      );

      if (result.success) {
        this.log.info(`Successfully finalized proposal: ${proposal.topic}`, {
          proposalId: proposal.id,
          winner: result.winner
        });
      } else {
        // Finalization failed (possibly Condorcet paradox / tie)
        this.log.warn('Finalization failed - flagging for manual review', {
          proposalId: proposal.id,
          error: result.error
        });

        await this.flagForManualReview(proposal, 'finalization_failed');
      }

    } catch (error: any) {
      this.log.error('Error finalizing proposal', {
        proposalId: proposal.id,
        error: error.message
      });

      // Flag for manual review on unexpected errors
      await this.flagForManualReview(proposal, 'unexpected_error');
    }
  }

  /**
   * Check if proposal has low participation
   */
  protected async checkLowParticipation(proposal: DecisionProposalEntity): Promise<boolean> {
    // Get total eligible voters (all AIs for now)
    const usersResult = await Commands.execute<any, any>(DATA_COMMANDS.LIST, {
      collection: COLLECTIONS.USERS,
      filter: { type: { $in: ['agent', 'persona'] } },
      limit: 100
    });

    if (!usersResult.success || !usersResult.items) {
      // Can't determine participation - be conservative and flag
      return true;
    }

    const totalEligible = usersResult.items.length;
    const totalVoted = proposal.votes.length;
    const participationRate = totalEligible > 0 ? totalVoted / totalEligible : 0;

    return participationRate < this.MIN_PARTICIPATION_RATE;
  }

  /**
   * Flag a proposal for manual review
   */
  protected async flagForManualReview(proposal: DecisionProposalEntity, reason: string): Promise<void> {
    try {
      // Update proposal status to require manual review
      await Commands.execute<any, any>(DATA_COMMANDS.UPDATE, {
        collection: COLLECTIONS.DECISION_PROPOSALS,
        id: proposal.id,
        data: {
          status: 'manual_review',
          metadata: {
            ...(proposal.metadata || {}),
            reviewReason: reason,
            flaggedAt: Date.now()
          }
        }
      });

      // Emit event for notifications
      Events.emit('governance:manual-review-required', {
        proposalId: proposal.id,
        proposalTopic: proposal.topic,
        reason,
        voteCount: proposal.votes.length
      });

      this.log.info('Flagged proposal for manual review', {
        proposalId: proposal.id,
        reason
      });

    } catch (error) {
      this.log.error('Failed to flag proposal for manual review', {
        proposalId: proposal.id,
        reason,
        error
      });
    }
  }

  /**
   * Shutdown the governance daemon
   */
  async shutdown(): Promise<void> {
    this.log.info('Shutting down governance daemon');
    this.isShuttingDown = true;

    if (this.schedulerInterval) {
      clearInterval(this.schedulerInterval);
      this.schedulerInterval = null;
    }

    await super.shutdown();
    this.log.info('Governance daemon shut down');
  }
}
