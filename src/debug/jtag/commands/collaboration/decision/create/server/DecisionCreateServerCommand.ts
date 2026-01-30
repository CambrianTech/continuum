/**
 * Decision Create Command - Server Implementation
 *
 * Create a new governance proposal with voting options
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { DecisionCreateParams, DecisionCreateResult } from '../shared/DecisionCreateTypes';
import { createDecisionCreateResultFromParams } from '../shared/DecisionCreateTypes';
import { DecisionEntity } from '@system/data/entities/DecisionEntity';
import type { DecisionOption } from '@system/data/entities/DecisionEntity';
import { UserIdentityResolver } from '@system/user/shared/UserIdentityResolver';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '@system/core/shared/Commands';
import { UserEntity } from '@system/data/entities/UserEntity';
import type { DataCreateParams } from '@commands/data/create/shared/DataCreateTypes';
import type { DataCreateResult } from '@commands/data/create/shared/DataCreateTypes';
import type { DataListParams } from '@commands/data/list/shared/DataListTypes';
import type { DataListResult } from '@commands/data/list/shared/DataListTypes';

import { DataCreate } from '../../../../data/create/shared/DataCreateTypes';
import { DataList } from '../../../../data/list/shared/DataListTypes';
export class DecisionCreateServerCommand extends CommandBase<DecisionCreateParams, DecisionCreateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Decision Create', context, subpath, commander);
  }

  async execute(params: DecisionCreateParams): Promise<DecisionCreateResult> {
    console.log('🔧 SERVER: Executing Decision Create', params);

    // Validate required parameters
    if (!params.proposalId || params.proposalId.trim() === '') {
      throw new ValidationError(
        'proposalId',
        `Missing required parameter 'proposalId'. ` +
        `Use the help tool with 'Decision Create' or see the Decision Create README for usage information.`
      );
    }

    if (!params.topic || params.topic.trim() === '') {
      throw new ValidationError(
        'topic',
        `Missing required parameter 'topic'. ` +
        `Use the help tool with 'Decision Create' or see the Decision Create README for usage information.`
      );
    }

    if (!params.rationale || params.rationale.trim() === '') {
      throw new ValidationError(
        'rationale',
        `Missing required parameter 'rationale'. ` +
        `Use the help tool with 'Decision Create' or see the Decision Create README for usage information.`
      );
    }

    if (!params.description || params.description.trim() === '') {
      throw new ValidationError(
        'description',
        `Missing required parameter 'description'. ` +
        `Use the help tool with 'Decision Create' or see the Decision Create README for usage information.`
      );
    }

    if (!params.options || params.options.length < 2) {
      throw new ValidationError(
        'options',
        `At least 2 options are required for a decision. ` +
        `Use the help tool with 'Decision Create' or see the Decision Create README for usage information.`
      );
    }

    // Validate each option has required fields
    for (const option of params.options) {
      if (!option.id || option.id.trim() === '') {
        throw new ValidationError('options', 'Each option must have an id');
      }
      if (!option.label || option.label.trim() === '') {
        throw new ValidationError('options', 'Each option must have a label');
      }
    }

    // Validate option IDs are unique
    const optionIds = params.options.map(opt => opt.id);
    const uniqueIds = new Set(optionIds);
    if (uniqueIds.size !== optionIds.length) {
      throw new ValidationError('options', 'Option IDs must be unique');
    }

    // Detect caller identity (Claude Code, Joel, etc.)
    const callerIdentity = await this.findCallerIdentity(params);

    // Create DecisionEntity
    const decision = new DecisionEntity();
    decision.proposalId = params.proposalId;
    decision.topic = params.topic;
    decision.rationale = params.rationale;
    decision.description = params.description;
    decision.options = params.options as readonly DecisionOption[];
    decision.tags = params.tags as readonly string[] | undefined;
    decision.proposedBy = callerIdentity.id;
    decision.proposedAt = new Date();
    decision.status = 'open';
    decision.votes = [];
    decision.visibility = (params.visibility as 'public' | 'private') || 'public';
    decision.requiredQuorum = params.requiredQuorum;
    decision.auditLog = [{
      timestamp: new Date().toISOString(),
      userId: callerIdentity.id,
      action: 'created',
      details: { topic: params.topic }
    }];

    if (params.votingDeadline) {
      decision.votingDeadline = new Date(params.votingDeadline);
    }

    // Validate entity
    const validation = decision.validate();
    if (!validation.success) {
      throw new ValidationError('decision', validation.error || 'Decision validation failed');
    }

    // Store in database using data/create command
    await DataCreate.execute<DecisionEntity>({
        collection: DecisionEntity.collection,
        data: decision,
        context: params.context,
        sessionId: params.sessionId
      }
    );

    // Return successful result
    return createDecisionCreateResultFromParams(params, {
      success: true,
      proposalId: decision.proposalId,
      proposedBy: decision.proposedBy,
      proposedAt: decision.proposedAt.toISOString(),
      status: decision.status,
    });
  }

  /**
   * Find the user identity of the caller (Claude Code, Joel, etc.)
   * Uses UserIdentityResolver to auto-detect calling process
   */
  private async findCallerIdentity(params: DecisionCreateParams): Promise<{ id: UUID; displayName: string }> {
    // Resolve caller identity (async)
    const identity = await UserIdentityResolver.resolve();
    const uniqueId = identity.uniqueId;

    // Find user by uniqueId in database using Commands.execute
    const result = await DataList.execute<UserEntity>({
        collection: UserEntity.collection,
        filter: { uniqueId },
        limit: 1,
        context: params.context,
        sessionId: params.sessionId
      }
    );

    if (!result.success || !result.items || result.items.length === 0) {
      throw new Error(`Caller identity not found in database: ${identity.displayName} (uniqueId: ${uniqueId})`);
    }

    return result.items[0];
  }
}
