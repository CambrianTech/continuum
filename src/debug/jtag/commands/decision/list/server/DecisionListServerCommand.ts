/**
 * Decision List Command - Server Implementation
 *
 * List all governance proposals with optional filtering
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
// import { ValidationError } from '../../../../system/core/types/ErrorTypes';  // Uncomment when adding validation
import type { DecisionListParams, DecisionListResult } from '../shared/DecisionListTypes';
import { createDecisionListResultFromParams } from '../shared/DecisionListTypes';

export class DecisionListServerCommand extends CommandBase<DecisionListParams, DecisionListResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Decision List', context, subpath, commander);
  }

  async execute(params: DecisionListParams): Promise<DecisionListResult> {
    console.log('🔧 SERVER: Executing Decision List', params);

    try {
      // Query proposals from database using Commands pattern
      const { Commands } = await import('../../../../system/core/shared/Commands');
      const { COLLECTIONS } = await import('../../../../system/shared/Constants');

      const limit = params.limit ?? 50;
      const offset = params.offset ?? 0;

      // Build filter based on optional status parameter
      const filter: any = {};
      if (params.status) {
        filter.status = params.status;
      }

      const listResult = await Commands.execute<any, any>('data/list', {
        collection: COLLECTIONS.DECISION_PROPOSALS,
        filter,
        limit,
        offset,
        orderBy: [{ field: 'sequenceNumber', direction: 'desc' }]  // Newest first
      });

      if (!listResult.success) {
        return createDecisionListResultFromParams(params, {
          success: false,
          proposals: [],
          total: 0,
          limit,
          offset
        });
      }

      const proposals = listResult.items || [];
      const total = listResult.total || proposals.length;

      return createDecisionListResultFromParams(params, {
        success: true,
        proposals: proposals as any,  // Type mismatch between DecisionEntity and DecisionProposalEntity
        total,
        limit,
        offset
      });

    } catch (error: any) {
      console.error('Error in decision/list:', error);
      return createDecisionListResultFromParams(params, {
        success: false,
        proposals: [],
        total: 0,
        limit: params.limit ?? 50,
        offset: params.offset ?? 0
      });
    }
  }
}
