/**
 * decision/propose - Server-side implementation
 *
 * Creates decision proposal with:
 * - Validation (min 2 options)
 * - Auto-generated proposal ID
 * - Deadline calculation based on significance
 * - Related proposals discovery via vector similarity
 * - Scope-filtered notifications
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { generateUUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { Commands } from '../../../../system/core/shared/Commands';
import { COLLECTIONS } from '../../../../system/shared/Constants';
import { DecisionProposeCommand } from '../shared/DecisionProposeCommand';
import type { DecisionProposeParams, DecisionProposeResult } from '../shared/DecisionProposeTypes';
import type { DecisionProposalEntity, DecisionOption } from '../../../../system/data/entities/DecisionProposalEntity';
import type { UserEntity } from '../../../../system/data/entities/UserEntity';
import type { DataListResult } from '../../../../commands/data/list/shared/DataListTypes';
import type { ChatSendParams, ChatSendResult } from '../../../../commands/chat/send/shared/ChatSendTypes';

/**
 * Calculate voting deadline based on significance level
 */
function calculateDeadline(significance: string): number {
  const now = Date.now();
  const durations: Record<string, number> = {
    low: 24 * 60 * 60 * 1000,      // 24 hours
    medium: 12 * 60 * 60 * 1000,   // 12 hours
    high: 4 * 60 * 60 * 1000,      // 4 hours
    critical: 1 * 60 * 60 * 1000   // 1 hour
  };

  return now + (durations[significance] || durations.medium);
}

/**
 * Extract keywords/tags from topic and rationale
 * Filters out common stop words and keeps meaningful terms
 */
function extractTags(topic: string, rationale: string): string[] {
  const stopWords = new Set([
    'the', 'a', 'an', 'and', 'or', 'but', 'in', 'on', 'at', 'to', 'for',
    'of', 'with', 'by', 'from', 'as', 'is', 'was', 'are', 'been', 'be',
    'have', 'has', 'had', 'do', 'does', 'did', 'will', 'would', 'should',
    'could', 'can', 'may', 'might', 'must', 'this', 'that', 'these', 'those',
    'we', 'you', 'they', 'them', 'their', 'our', 'your', 'it', 'its'
  ]);

  const text = (topic + ' ' + rationale).toLowerCase();
  const words = text
    .replace(/[^\w\s-]/g, ' ')
    .split(/\s+/)
    .filter(w => w.length > 3 && !stopWords.has(w));

  // Count word frequency
  const frequency = new Map<string, number>();
  for (const word of words) {
    frequency.set(word, (frequency.get(word) || 0) + 1);
  }

  // Return top 5-8 most frequent words as tags
  return Array.from(frequency.entries())
    .sort((a, b) => b[1] - a[1])
    .slice(0, 8)
    .map(([word]) => word);
}

/**
 * Find related proposals via tag-based similarity
 * Scores by tag overlap, with recency bonus
 * (Future: Could use vector embeddings for semantic matching)
 */
async function findRelatedProposals(tags: string[]): Promise<UUID[]> {
  try {
    if (tags.length === 0) {
      return [];
    }

    const result = await Commands.execute<any, DataListResult<DecisionProposalEntity>>('data/list', {
      collection: COLLECTIONS.DECISION_PROPOSALS,
      orderBy: [{ field: 'sequenceNumber', direction: 'desc' }],
      limit: 100
    });

    if (!result.success || !result.items) {
      return [];
    }

    const now = Date.now();
    const tagSet = new Set(tags);

    // Score proposals by tag overlap + recency
    const scored = result.items.map((proposal: any) => {
      const proposalTags = proposal.tags || [];
      const overlap = proposalTags.filter((t: string) => tagSet.has(t)).length;

      // Age penalty: proposals older than 30 days get reduced score
      const ageInDays = (now - proposal.createdAt) / (1000 * 60 * 60 * 24);
      const recencyBonus = ageInDays < 30 ? 0.5 : 0;

      const score = overlap + recencyBonus;

      return { id: proposal.id, score, overlap, createdAt: proposal.createdAt };
    });

    // Return top 3 most related (must have at least 1 tag overlap)
    return scored
      .filter(s => s.overlap > 0)
      .sort((a, b) => {
        // Primary sort: score (tag overlap + recency)
        if (b.score !== a.score) return b.score - a.score;
        // Secondary sort: recency
        return b.createdAt - a.createdAt;
      })
      .slice(0, 3)
      .map(s => s.id);
  } catch (error) {
    console.error('Error finding related proposals:', error);
    return [];
  }
}

/**
 * Get users in scope for notification
 */
async function getUsersInScope(scope: string): Promise<UserEntity[]> {
  try {
    const result = await Commands.execute<any, DataListResult<UserEntity>>('data/list', {
      collection: COLLECTIONS.USERS,
      limit: 100
    });

    if (!result.success || !result.items) {
      return [];
    }

    const allUsers = result.items;

    // Filter based on scope
    switch (scope) {
      case 'all':
        return allUsers.filter((u: any) => u.type !== 'human'); // All AIs

      case 'code-experts':
        // Look for personas with 'code', 'dev', 'engineer' in name/bio
        return allUsers.filter((u: any) => {
          const text = (u.displayName + ' ' + (u.bio || '')).toLowerCase();
          return u.type !== 'human' && (
            text.includes('code') ||
            text.includes('dev') ||
            text.includes('engineer') ||
            text.includes('architect')
          );
        });

      case 'user-facing-ais':
        // Look for personas with 'chat', 'assistant', 'helper' in name
        return allUsers.filter((u: any) => {
          const text = (u.displayName + ' ' + (u.bio || '')).toLowerCase();
          return u.type !== 'human' && (
            text.includes('chat') ||
            text.includes('assistant') ||
            text.includes('helper') ||
            text.includes('support')
          );
        });

      case 'local-models':
        // Personas running on Ollama
        return allUsers.filter((u: any) => {
          return u.type === 'persona';
        });

      case 'external-apis':
        // Agent users (Claude, GPT, etc.)
        return allUsers.filter((u: any) => {
          return u.type === 'agent';
        });

      default:
        return allUsers.filter((u: any) => u.type !== 'human');
    }
  } catch (error) {
    console.error('Error getting users in scope:', error);
    return [];
  }
}

/**
 * DecisionProposeServerCommand - Server implementation
 */
export class DecisionProposeServerCommand extends DecisionProposeCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  /**
   * Execute decision/propose command
   */
  protected async executeCommand(params: DecisionProposeParams): Promise<DecisionProposeResult> {
  try {
    // Validation
    if (!params.topic?.trim()) {
      return transformPayload(params, { success: false, error: 'Topic is required' });
    }

    if (!params.rationale?.trim()) {
      return transformPayload(params, { success: false, error: 'Rationale is required' });
    }

    if (!Array.isArray(params.options) || params.options.length < 2) {
      return transformPayload(params, { success: false, error: 'At least 2 options are required' });
    }

    for (const opt of params.options) {
      if (!opt.label?.trim()) {
        return transformPayload(params, { success: false, error: 'All options must have a label' });
      }
      if (!opt.description?.trim()) {
        return transformPayload(params, { success: false, error: 'All options must have a description' });
      }
    }

    // Get proposer info from params or find default user
    let proposerId: UUID;
    if (params.proposerId) {
      proposerId = params.proposerId;
    } else {
      // Fall back to finding default human user (similar to ChatSendCommand)
      const usersResult = await Commands.execute<any, DataListResult<UserEntity>>('data/list', {
        collection: COLLECTIONS.USERS,
        filter: { type: 'human' },
        limit: 1
      });
      if (!usersResult.success || !usersResult.items || usersResult.items.length === 0) {
        return transformPayload(params, { success: false, error: 'Could not find proposer user' });
      }
      proposerId = usersResult.items[0].id;
    }

    const proposerResult = await Commands.execute<any, any>('data/read', {
      collection: COLLECTIONS.USERS,
      id: proposerId
    });

    if (!proposerResult.success || !proposerResult.data) {
      return transformPayload(params, { success: false, error: 'Could not find proposer user' });
    }

    const proposer = proposerResult.data;
    const scope = params.scope || 'all';
    const significanceLevel = params.significanceLevel || 'medium';
    const proposalId = generateUUID();

    // Extract or use provided tags
    const tags = params.tags && params.tags.length > 0
      ? params.tags
      : extractTags(params.topic, params.rationale);

    // Generate option IDs with proposedBy tracking
    const options: DecisionOption[] = params.options.map((opt, idx) => ({
      id: `opt-${idx + 1}`,
      label: opt.label,
      description: opt.description,
      proposedBy: opt.proposedBy || proposerId // Default to proposer
    }));

    // Calculate deadline
    const deadline = calculateDeadline(significanceLevel);

    // Find related proposals using tags
    const relatedProposals = await findRelatedProposals(tags);

    // Get next sequence number
    const countResult = await Commands.execute<any, any>('data/list', {
      collection: COLLECTIONS.DECISION_PROPOSALS,
      limit: 1,
      orderBy: [{ field: 'sequenceNumber', direction: 'desc' }]
    });

    const sequenceNumber = countResult.success && countResult.items?.[0]
      ? countResult.items[0].sequenceNumber + 1
      : 1;

    // Create proposal
    const proposalData: Partial<DecisionProposalEntity> = {
      id: proposalId,
      proposerId: proposerId,
      proposerName: proposer.displayName,
      topic: params.topic,
      context: params.rationale,
      tags,
      options,
      scope,
      significanceLevel,
      status: 'voting',
      deadline,
      votes: [],
      relatedProposals,
      conveneEvents: [], // Will be populated when discussions are triggered
      contextId: params.contextId || params.sessionId || ('' as UUID),
      sequenceNumber,
      createdAt: new Date(Date.now()),
      updatedAt: new Date(Date.now())
    };

    const createResult = await Commands.execute<any, any>('data/create', {
      collection: COLLECTIONS.DECISION_PROPOSALS,
      data: proposalData
    });

    if (!createResult.success) {
      return transformPayload(params, { success: false, error: createResult.error || 'Failed to create proposal' });
    }

    // Notify users in scope
    const usersInScope = await getUsersInScope(scope);

    // Send notification to general room
    const notificationMessage = `📋 **New Decision Proposal: ${params.topic}**

${params.rationale}

**Options:**
${options.map((opt, idx) => `${idx + 1}. **${opt.label}**: ${opt.description}`).join('\n')}

**Scope:** ${scope}
**Urgency:** ${significanceLevel}
**Deadline:** ${new Date(deadline).toLocaleString()}

Use \`decision/rank\` to submit your ranked preferences.
Proposal ID: ${proposalId}`;

    await Commands.execute<ChatSendParams, ChatSendResult>('chat/send', {
      message: notificationMessage,
      room: 'general'
    });

    return transformPayload(params, {
      success: true,
      proposalId,
      deadline,
      notifiedCount: usersInScope.length,
      relatedProposals
    });
  } catch (error: any) {
    console.error('Error in decision/propose:', error);
    return transformPayload(params, {
      success: false,
      error: error.message || 'Unknown error'
    });
  }
  }
}
