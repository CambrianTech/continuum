/**
 * Governance Notifications
 *
 * Subscribes to governance events (voting, finalization) and posts
 * notifications to chat so the AI team can see real-time updates
 */

import { Events } from '../core/shared/Events';
import { Commands } from '../core/shared/Commands';
import type { ChatSendParams, ChatSendResult } from '../../commands/chat/send/shared/ChatSendTypes';
import { Logger } from '../core/logging/Logger';

const log = Logger.create('GovernanceNotifications', 'governance');

/**
 * Initialize governance notification subscriptions
 * Should be called during system startup
 */
export function initializeGovernanceNotifications(): void {
  log.info('Initializing governance notification subscriptions');

  // Subscribe to vote events
  Events.subscribe('decision:voted', async (data: any) => {
    try {
      const { proposalTopic, voterName, totalVotes, isUpdate } = data;

      const action = isUpdate ? 'updated their vote' : 'voted';
      const message = `🗳️ **${voterName}** ${action} on **${proposalTopic}** (${totalVotes} vote${totalVotes === 1 ? '' : 's'} cast)`;

      await Commands.execute<ChatSendParams, ChatSendResult>('chat/send', {
        message,
        room: 'general'
      });

      log.debug('Posted vote notification to chat', { voterName, proposalTopic });
    } catch (error) {
      log.error('Failed to post vote notification', { error });
    }
  });

  // Subscribe to finalization events
  Events.subscribe('decision:finalized', async (data: any) => {
    try {
      const { proposalTopic, winnerLabel, totalVotes, participationRate, consensusStrength } = data;

      const message = `🏆 **Decision Finalized: ${proposalTopic}**

**Winner:** ${winnerLabel}
**Votes:** ${totalVotes}
**Turnout:** ${participationRate.toFixed(1)}%
**Consensus:** ${(consensusStrength * 100).toFixed(0)}% pairwise victories`;

      await Commands.execute<ChatSendParams, ChatSendResult>('chat/send', {
        message,
        room: 'general'
      });

      log.info('Posted finalization notification to chat', { proposalTopic, winnerLabel });
    } catch (error) {
      log.error('Failed to post finalization notification', { error });
    }
  });

  log.info('Governance notification subscriptions initialized');
}
