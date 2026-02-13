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

import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { generateUUID, toShortId } from '@system/core/types/CrossPlatformUUID';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { Commands } from '@system/core/shared/Commands';
import { COLLECTIONS } from '@system/shared/Constants';
import { DecisionProposeCommand } from '../shared/DecisionProposeCommand';
import type { DecisionProposeParams, DecisionProposeResult } from '../shared/DecisionProposeTypes';

// Caller identity now comes from context.userId - no need for callerId/personaId injection
import type { DecisionProposalEntity, DecisionOption } from '@system/data/entities/DecisionProposalEntity';
import type { UserEntity } from '@system/data/entities/UserEntity';
import type { DataListParams, DataListResult } from '@commands/data/list/shared/DataListTypes';
import type { DataReadParams, DataReadResult } from '@commands/data/read/shared/DataReadTypes';
import type { DataCreateParams, DataCreateResult } from '@commands/data/create/shared/DataCreateTypes';
import type { ChatSendParams, ChatSendResult } from '@commands/collaboration/chat/send/shared/ChatSendTypes';
import { Logger } from '@system/core/logging/Logger';
import { UserIdentityResolver } from '@system/user/shared/UserIdentityResolver';

import { DataList } from '../../../../data/list/shared/DataListTypes';
import { DataRead } from '../../../../data/read/shared/DataReadTypes';
import { DataCreate } from '../../../../data/create/shared/DataCreateTypes';
import { ChatSend } from '../../../chat/send/shared/ChatSendTypes';
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

    const result = await DataList.execute<DecisionProposalEntity>({
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
    const result = await DataList.execute<UserEntity>({
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
        // Personas running on local inference (Candle)
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
  private log = Logger.create('DecisionProposeServerCommand', 'tools');

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  /**
   * Execute decision/propose command
   */
  protected async executeCommand(params: DecisionProposeParams): Promise<DecisionProposeResult> {
  try {
    // Parse JSON strings if present (AIs may pass JSON strings instead of arrays)
    if (typeof params.options === 'string') {
      this.log.warn('Parameter format conversion: options received as JSON string instead of array', {
        command: 'decision/propose',
        topic: params.topic,
        sessionId: params.sessionId
      });
      try {
        params.options = JSON.parse(params.options);
      } catch (e) {
        return transformPayload(params, { success: false, error: 'options must be a valid JSON array' });
      }
    }

    if (typeof params.tags === 'string') {
      this.log.warn('Parameter format conversion: tags received as JSON string instead of array', {
        command: 'decision/propose',
        topic: params.topic,
        sessionId: params.sessionId
      });
      try {
        params.tags = JSON.parse(params.tags);
      } catch (e) {
        return transformPayload(params, { success: false, error: 'tags must be a valid JSON array' });
      }
    }

    // Handle string arrays for options - convert to proper objects
    if (Array.isArray(params.options) && params.options.length > 0 && typeof params.options[0] === 'string') {
      this.log.warn('Parameter format conversion: options received as string array, converting to object array', {
        command: 'decision/propose',
        topic: params.topic,
        sessionId: params.sessionId
      });
      const stringOptions = params.options as unknown as string[];
      params.options = stringOptions.map((optionStr, idx) => {
        // Try to split on colon to extract label and description
        const colonIndex = optionStr.indexOf(':');
        if (colonIndex > 0) {
          return {
            label: optionStr.substring(0, colonIndex).trim(),
            description: optionStr.substring(colonIndex + 1).trim()
          };
        }
        // Fallback: use entire string as label with generic description
        return {
          label: `Option ${idx + 1}`,
          description: optionStr.trim()
        };
      });
    }

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

    // Get proposer info - auto-detect caller identity
    // Priority: 1) context.userId (PersonaUsers), 2) UserIdentityResolver (CLI)
    let proposerId: UUID;
    let proposerName: string;

    if (params.context?.userId) {
      // FIRST: Check context.userId (PersonaUsers set this)
      const proposerResult = await DataRead.execute<UserEntity>({
        collection: COLLECTIONS.USERS,
        id: params.context.userId
      });

      if (!proposerResult.success || !proposerResult.data) {
        return transformPayload(params, { success: false, error: 'Could not find proposer user from context' });
      }

      proposerId = params.context.userId;
      proposerName = proposerResult.data.displayName;
      this.log.debug('Using context.userId for proposer', { proposerId, proposerName });
    } else {
      // FALLBACK: Auto-detect caller identity using UserIdentityResolver (CLI calls)
      const identity = await UserIdentityResolver.resolve();

      this.log.debug('Auto-detected proposer identity', {
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

      proposerId = identity.userId;
      proposerName = identity.displayName;
    }
    const scope = params.scope || 'all';
    const significanceLevel = params.significanceLevel || 'medium';
    const proposalId = generateUUID();

    // Extract or use provided tags
    const tags = params.tags && params.tags.length > 0
      ? params.tags
      : extractTags(params.topic, params.rationale);

    // Generate option IDs with proposedBy tracking (UUIDs for #abc123 short ID voting)
    const options: DecisionOption[] = params.options.map((opt) => ({
      id: generateUUID(),
      label: opt.label,
      description: opt.description,
      proposedBy: opt.proposedBy || proposerId // Default to proposer
    }));

    // Calculate deadline
    const deadline = calculateDeadline(significanceLevel);

    // Find related proposals using tags
    const relatedProposals = await findRelatedProposals(tags);

    // Get next sequence number
    const countResult = await DataList.execute<DecisionProposalEntity>({
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
      proposerName: proposerName,
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

    const createResult = await DataCreate.execute<DecisionProposalEntity>({
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

    await ChatSend.execute({
      message: notificationMessage,
      room: 'general',
      senderId: proposerId  // Use proposer's identity, not caller's context
    });

    return transformPayload(params, {
      success: true,
      proposalId,
      shortId: toShortId(proposalId), // Human-friendly reference (#abc123)
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
