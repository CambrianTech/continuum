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

    // Validate required parameters
    // NOTE: Commands should THROW errors when validation fails, not catch and return success:false
    // This demonstrates BEST PRACTICE error handling for command templates
    //
    // Example validation for a required parameter:
    // if (!params.yourRequiredParam || params.yourRequiredParam.trim() === '') {
    //   throw new ValidationError(
    //     'yourRequiredParam',
    //     `Missing required parameter 'yourRequiredParam'. ` +
    //     `Use the help tool with 'Decision List' or see the Decision List README for usage information.`
    //   );
    // }

    // TODO: Implement your command logic here
    // Add validation for each required parameter following the pattern above
    // The error message should:
    // 1. Reference the help tool generically (works for both jtag CLI and Persona tools)
    // 2. Reference the command README using the command name
    // 3. Be clear about what's missing or invalid

    // Return successful result with all required fields
    // NOTE: createResultFromParams requires ALL result fields (context/sessionId inherited from params)
    return createDecisionListResultFromParams(params, {
      success: true, // Whether the query succeeded
      proposals: [], // Array of matching proposals - TODO: Query from data/list
      total: 0, // Total number of matching proposals (before pagination) - TODO: Calculate from query
      limit: params.limit ?? 50, // The limit that was applied
      offset: params.offset ?? 0, // The offset that was applied
    });
  }
}
