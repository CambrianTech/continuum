/**
 * Social Propose Command - Server Implementation
 *
 * Democratic governance for shared social media accounts.
 * Proposals stored as Handles, auto-execute when vote threshold met.
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { SocialProposeBaseCommand } from '../shared/SocialProposeCommand';
import type {
  SocialProposeParams,
  SocialProposeResult,
  ProposalData,
  ProposalRecord,
  ProposalVote,
  ProposalAction,
  ProposalStatus,
} from '../shared/SocialProposeTypes';
import {
  PROPOSAL_THRESHOLDS,
  PROPOSAL_TTL_MS,
  PROPOSAL_HANDLE_TYPE,
} from '../shared/SocialProposeTypes';
import { Handles } from '@system/core/shared/Handles';
import type { HandleRecord } from '@system/core/types/Handle';
import { loadSocialContext, resolvePersonaId } from '@system/social/server/SocialCommandHelper';
import { SocialEngage } from '@commands/social/engage/shared/SocialEngageTypes';
import { SocialPost } from '@commands/social/post/shared/SocialPostTypes';
import { SocialComment } from '@commands/social/comment/shared/SocialCommentTypes';
import { DataList } from '@commands/data/list/shared/DataListTypes';
import { UserEntity } from '@system/data/entities/UserEntity';


export class SocialProposeServerCommand extends SocialProposeBaseCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeSocialPropose(params: SocialProposeParams): Promise<SocialProposeResult> {
    const mode = params.mode ?? 'list';

    switch (mode) {
      case 'create':
        return this.handleCreate(params);
      case 'vote':
        return this.handleVote(params);
      case 'list':
        return this.handleList(params);
      case 'view':
        return this.handleView(params);
      default:
        throw new Error(`Unknown propose mode: ${mode}. Valid: create, vote, list, view`);
    }
  }

  // ============ Create ============

  private async handleCreate(params: SocialProposeParams): Promise<SocialProposeResult> {
    const { platform, action, target, reason } = params;

    if (!platform) throw new Error('platform is required for proposals');
    if (!action) throw new Error('action is required (follow, post, comment, vote, subscribe, unsubscribe)');
    if (!reason) throw new Error('reason is required — explain why the community should approve this');

    const validActions: ProposalAction[] = ['follow', 'unfollow', 'post', 'comment', 'vote', 'subscribe', 'unsubscribe'];
    if (!validActions.includes(action)) {
      throw new Error(`Invalid action: ${action}. Valid: ${validActions.join(', ')}`);
    }

    // Resolve nominator
    const personaId = await resolvePersonaId(params.personaId, params);
    const persona = await this.lookupPersona(personaId, params);

    // Build action params that will be used for execution
    const actionParams = this.buildActionParams(params);

    // Validate action-specific requirements
    this.validateActionParams(action, target, params);

    const threshold = PROPOSAL_THRESHOLDS[action];

    const proposalData: ProposalData = {
      action,
      platform,
      target,
      reason,
      nominatedBy: personaId,
      nominatorName: persona.displayName,
      votes: [{
        personaId,
        personaName: persona.displayName,
        direction: 'up',
        timestamp: new Date().toISOString(),
      }],
      threshold,
      actionParams,
    };

    // Threshold of 0 means auto-approve — execute immediately without voting
    if (threshold === 0) {
      const handle = await Handles.create(
        PROPOSAL_HANDLE_TYPE,
        proposalData,
        personaId,
        PROPOSAL_TTL_MS,
      );
      const record = this.handleToProposal(handle, proposalData);
      return this.executeProposal(handle, proposalData, params, record);
    }

    // Create handle for the proposal
    const handle = await Handles.create(
      PROPOSAL_HANDLE_TYPE,
      proposalData,
      personaId,
      PROPOSAL_TTL_MS,
    );

    const record = this.handleToProposal(handle, proposalData);
    const votesNeeded = threshold - 1; // Nominator auto-votes up

    // Check if nominator's single vote meets threshold (e.g., vote action needs 2)
    if (proposalData.votes.filter(v => v.direction === 'up').length >= threshold) {
      return this.executeProposal(handle, proposalData, params, record);
    }

    return transformPayload(params, {
      success: true,
      message: `Proposal created: ${action} ${target ?? ''} on ${platform}`,
      summary: this.formatProposalSummary(record, votesNeeded),
      proposal: record,
      executed: false,
    });
  }

  // ============ Vote ============

  private async handleVote(params: SocialProposeParams): Promise<SocialProposeResult> {
    const { proposalId, direction } = params;

    if (!proposalId) throw new Error('proposalId is required');
    if (!direction || !['up', 'down'].includes(direction)) {
      throw new Error('direction is required (up or down)');
    }

    // Resolve voter
    const personaId = await resolvePersonaId(params.personaId, params);
    const persona = await this.lookupPersona(personaId, params);

    // Load proposal handle
    const handle = await Handles.resolve(proposalId);
    if (!handle) {
      throw new Error(`Proposal not found: ${proposalId}`);
    }
    if (handle.type !== PROPOSAL_HANDLE_TYPE) {
      throw new Error(`Handle ${proposalId} is not a proposal (type: ${handle.type})`);
    }
    if (handle.status !== 'pending') {
      throw new Error(`Proposal ${proposalId} is not open for voting (status: ${handle.status})`);
    }

    const proposalData = handle.params as ProposalData;

    // Check if already voted
    const existingVote = proposalData.votes.find(v => v.personaId === personaId);
    if (existingVote) {
      if (existingVote.direction === direction) {
        throw new Error(`You already voted ${direction} on this proposal`);
      }
      // Change vote direction
      existingVote.direction = direction;
      existingVote.timestamp = new Date().toISOString();
    } else {
      // New vote
      proposalData.votes.push({
        personaId,
        personaName: persona.displayName,
        direction,
        timestamp: new Date().toISOString(),
      });
    }

    // Update the handle with new vote data
    await Handles._updateStatus(handle.id, 'pending', { params: proposalData });

    const record = this.handleToProposal(handle, proposalData);
    const upVotes = proposalData.votes.filter(v => v.direction === 'up').length;
    const votesNeeded = proposalData.threshold - upVotes;

    // Check if threshold met
    if (upVotes >= proposalData.threshold) {
      return this.executeProposal(handle, proposalData, params, record);
    }

    // Check if mathematically impossible (too many downvotes)
    const downVotes = proposalData.votes.filter(v => v.direction === 'down').length;
    const totalPossibleVoters = 12; // Approximate active persona count
    const maxPossibleUp = upVotes + (totalPossibleVoters - proposalData.votes.length);
    if (maxPossibleUp < proposalData.threshold) {
      await Handles.markFailed(handle.id, 'Rejected: insufficient support');
      record.status = 'rejected';
      return transformPayload(params, {
        success: true,
        message: `Proposal rejected: not enough possible votes remaining`,
        summary: this.formatProposalSummary(record, 0),
        proposal: record,
        executed: false,
      });
    }

    return transformPayload(params, {
      success: true,
      message: `Voted ${direction} on proposal #${handle.shortId}`,
      summary: this.formatProposalSummary(record, Math.max(0, votesNeeded)),
      proposal: record,
      executed: false,
    });
  }

  // ============ List ============

  private async handleList(params: SocialProposeParams): Promise<SocialProposeResult> {
    const limit = params.limit ?? 20;

    // Fetch proposal handles
    let handles: HandleRecord[];
    if (params.status === 'pending') {
      handles = await Handles.listActive(PROPOSAL_HANDLE_TYPE, limit);
    } else {
      handles = await Handles.listByType(PROPOSAL_HANDLE_TYPE, limit);
    }

    // Convert to proposals
    const proposals = handles.map(h => {
      const data = h.params as ProposalData;
      return this.handleToProposal(h, data);
    });

    // Filter by status if specified (for non-pending)
    const filtered = params.status && params.status !== 'pending'
      ? proposals.filter(p => p.status === params.status)
      : proposals;

    const lines = filtered.map((p, i) => {
      const upVotes = p.voteSummary.up;
      const bar = '█'.repeat(upVotes) + '░'.repeat(Math.max(0, p.threshold - upVotes));
      const statusTag = p.status === 'pending' ? '🗳️' :
        p.status === 'executed' ? '✅' :
        p.status === 'rejected' ? '❌' :
        p.status === 'expired' ? '⏰' : '?';
      return `${statusTag} #${p.shortId} [${bar}] ${upVotes}/${p.threshold} — ${p.action} ${p.target ?? ''} (${p.nominatorName}: "${p.reason}")`;
    });

    return transformPayload(params, {
      success: true,
      message: `${filtered.length} proposal(s) found`,
      summary: filtered.length > 0
        ? `**Proposals:**\n${lines.join('\n')}\n\nVote: social/propose --mode=vote --proposalId=<id> --direction=up`
        : 'No proposals found. Create one: social/propose --mode=create --action=follow --target=<agent> --reason="why"',
      proposals: filtered,
    });
  }

  // ============ View ============

  private async handleView(params: SocialProposeParams): Promise<SocialProposeResult> {
    const { proposalId } = params;
    if (!proposalId) throw new Error('proposalId is required');

    const handle = await Handles.resolve(proposalId);
    if (!handle) throw new Error(`Proposal not found: ${proposalId}`);
    if (handle.type !== PROPOSAL_HANDLE_TYPE) {
      throw new Error(`Handle ${proposalId} is not a proposal`);
    }

    const data = handle.params as ProposalData;
    const record = this.handleToProposal(handle, data);

    const voteLines = data.votes.map(v => {
      const icon = v.direction === 'up' ? '👍' : '👎';
      return `  ${icon} ${v.personaName} (${v.direction}) — ${new Date(v.timestamp).toLocaleTimeString()}`;
    });

    const summary = [
      `**Proposal #${record.shortId}** — ${record.action} ${record.target ?? ''}`,
      `Platform: ${record.platform}`,
      `Status: ${record.status}`,
      `Reason: "${record.reason}"`,
      `Nominated by: ${record.nominatorName}`,
      `Threshold: ${record.threshold} votes needed`,
      `Votes (${record.voteSummary.up} up, ${record.voteSummary.down} down):`,
      ...voteLines,
      '',
      record.status === 'pending'
        ? `Vote: social/propose --mode=vote --proposalId=${record.shortId} --direction=up`
        : `This proposal is ${record.status}.`,
    ].join('\n');

    return transformPayload(params, {
      success: true,
      message: `Proposal #${record.shortId}: ${record.status}`,
      summary,
      proposal: record,
    });
  }

  // ============ Auto-Execute ============

  private async executeProposal(
    handle: HandleRecord,
    data: ProposalData,
    params: SocialProposeParams,
    record: ProposalRecord,
  ): Promise<SocialProposeResult> {
    await Handles.markProcessing(handle.id);

    try {
      const result = await this.executeAction(data, params);

      await Handles.markComplete(handle.id, {
        executed: true,
        executionResult: result,
        executedAt: new Date().toISOString(),
      });

      record.status = 'executed';

      return transformPayload(params, {
        success: true,
        message: `Proposal approved and executed: ${data.action} ${data.target ?? ''} on ${data.platform}`,
        summary: `**Proposal #${handle.shortId} APPROVED** — threshold met (${data.votes.filter(v => v.direction === 'up').length}/${data.threshold})\nAction: ${data.action} ${data.target ?? ''}\nResult: ${JSON.stringify(result)}`,
        proposal: record,
        executed: true,
        executionResult: result,
      });
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      await Handles.markFailed(handle.id, msg);
      record.status = 'rejected';

      return transformPayload(params, {
        success: false,
        message: `Proposal approved but execution failed: ${msg}`,
        proposal: record,
        executed: false,
      });
    }
  }

  private async executeAction(data: ProposalData, params: SocialProposeParams): Promise<unknown> {
    const { action, platform, target, actionParams } = data;

    switch (action) {
      case 'follow':
        return SocialEngage.execute({
          platform,
          action: 'follow',
          target: target!,
          context: params.context,
          sessionId: params.sessionId,
        });

      case 'unfollow':
        return SocialEngage.execute({
          platform,
          action: 'unfollow',
          target: target!,
          context: params.context,
          sessionId: params.sessionId,
        });

      case 'subscribe':
        return SocialEngage.execute({
          platform,
          action: 'subscribe',
          target: target!,
          context: params.context,
          sessionId: params.sessionId,
        });

      case 'unsubscribe':
        return SocialEngage.execute({
          platform,
          action: 'unsubscribe',
          target: target!,
          context: params.context,
          sessionId: params.sessionId,
        });

      case 'vote':
        return SocialEngage.execute({
          platform,
          action: 'vote',
          target: target!,
          targetType: (actionParams.targetType as 'post' | 'comment') ?? 'post',
          direction: (actionParams.voteDirection as 'up' | 'down') ?? 'up',
          context: params.context,
          sessionId: params.sessionId,
        });

      case 'post':
        return SocialPost.execute({
          platform,
          title: actionParams.title as string,
          content: actionParams.content as string,
          community: actionParams.community as string | undefined,
          context: params.context,
          sessionId: params.sessionId,
        });

      case 'comment':
        return SocialComment.execute({
          platform,
          postId: actionParams.postId as string,
          content: actionParams.commentContent as string ?? actionParams.content as string,
          parentId: actionParams.parentId as string | undefined,
          context: params.context,
          sessionId: params.sessionId,
        });

      default:
        throw new Error(`Cannot execute action: ${action}`);
    }
  }

  // ============ Helpers ============

  private buildActionParams(params: SocialProposeParams): Record<string, unknown> {
    const ap: Record<string, unknown> = {};
    if (params.title) ap.title = params.title;
    if (params.content) ap.content = params.content;
    if (params.community) ap.community = params.community;
    if (params.postId) ap.postId = params.postId;
    if (params.commentContent) ap.commentContent = params.commentContent;
    if (params.voteDirection) ap.voteDirection = params.voteDirection;
    if (params.targetType) ap.targetType = params.targetType;
    return ap;
  }

  private validateActionParams(action: ProposalAction, target: string | undefined, params: SocialProposeParams): void {
    switch (action) {
      case 'follow':
      case 'unfollow':
        if (!target) throw new Error(`${action} requires --target (agent username)`);
        break;
      case 'subscribe':
      case 'unsubscribe':
        if (!target) throw new Error(`${action} requires --target (community name)`);
        break;
      case 'vote':
        if (!target) throw new Error('vote requires --target (post or comment ID)');
        break;
      case 'post':
        if (!params.title || !params.content) throw new Error('post requires --title and --content');
        break;
      case 'comment':
        if (!params.postId) throw new Error('comment requires --postId');
        if (!params.content && !params.commentContent) throw new Error('comment requires --content or --commentContent');
        break;
    }
  }

  private handleToProposal(handle: HandleRecord, data: ProposalData): ProposalRecord {
    const upVotes = data.votes.filter(v => v.direction === 'up').length;
    const downVotes = data.votes.filter(v => v.direction === 'down').length;

    let status: ProposalStatus;
    switch (handle.status) {
      case 'pending': status = 'pending'; break;
      case 'processing': status = 'approved'; break;
      case 'complete': status = 'executed'; break;
      case 'failed': status = 'rejected'; break;
      case 'expired': status = 'expired'; break;
      case 'cancelled': status = 'rejected'; break;
      default: status = 'pending';
    }

    return {
      id: handle.id,
      shortId: handle.shortId,
      action: data.action,
      platform: data.platform,
      target: data.target,
      reason: data.reason,
      nominatedBy: data.nominatedBy,
      nominatorName: data.nominatorName,
      votes: data.votes,
      voteSummary: { up: upVotes, down: downVotes, total: data.votes.length },
      threshold: data.threshold,
      status,
      createdAt: handle.createdAt.toISOString(),
      expiresAt: handle.expiresAt?.toISOString(),
    };
  }

  private formatProposalSummary(record: ProposalRecord, votesNeeded: number): string {
    const bar = '█'.repeat(record.voteSummary.up) + '░'.repeat(Math.max(0, votesNeeded));
    return [
      `**Proposal #${record.shortId}** — ${record.action} ${record.target ?? ''}`,
      `Reason: "${record.reason}"`,
      `Progress: [${bar}] ${record.voteSummary.up}/${record.threshold} votes`,
      votesNeeded > 0
        ? `Need ${votesNeeded} more vote(s) to approve.`
        : 'Threshold met!',
      `Vote: social/propose --mode=vote --proposalId=${record.shortId} --direction=up`,
    ].join('\n');
  }

  private async lookupPersona(
    personaId: UUID,
    params: SocialProposeParams,
  ): Promise<{ displayName: string; uniqueId: string }> {
    const result = await DataList.execute<UserEntity>({
      collection: UserEntity.collection,
      filter: { id: personaId },
      limit: 1,
      context: params.context,
      sessionId: params.sessionId,
    });

    if (!result.success || !result.items?.length) {
      throw new Error(`Persona not found: ${personaId}`);
    }

    return {
      displayName: result.items[0].displayName,
      uniqueId: result.items[0].uniqueId,
    };
  }
}
