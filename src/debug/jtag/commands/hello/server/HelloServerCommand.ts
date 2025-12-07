/**
 * Hello Command - Server Implementation
 *
 * Simple hello world command for testing
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
// import { ValidationError } from '../../../system/core/types/ErrorTypes';  // Uncomment when adding validation
import type { HelloParams, HelloResult } from '../shared/HelloTypes';
import { createHelloResultFromParams } from '../shared/HelloTypes';

export class HelloServerCommand extends CommandBase<HelloParams, HelloResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Hello', context, subpath, commander);
  }

  async execute(params: HelloParams): Promise<HelloResult> {
    console.log('🔧 SERVER: Executing Hello', params);

    // Validate required parameters
    // NOTE: Commands should THROW errors when validation fails, not catch and return success:false
    // This demonstrates BEST PRACTICE error handling for command templates
    //
    // Example validation for a required parameter:
    // if (!params.yourRequiredParam || params.yourRequiredParam.trim() === '') {
    //   throw new ValidationError(
    //     'yourRequiredParam',
    //     `Missing required parameter 'yourRequiredParam'. ` +
    //     `Use the help tool with 'Hello' or see the Hello README for usage information.`
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
    return createHelloResultFromParams(params, {
      success: true,
      message: 'TODO: Hello world message', // Hello world message
    });
  }
}
