/**
 * {{COMMAND_NAME}} Command - Server Implementation
 *
 * {{DESCRIPTION}}
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
// import { ValidationError } from '../../../system/core/types/ErrorTypes';  // Uncomment when adding validation
import type { {{CLASS_NAME}}Params, {{CLASS_NAME}}Result } from '../shared/{{CLASS_NAME}}Types';
import { create{{CLASS_NAME}}ResultFromParams } from '../shared/{{CLASS_NAME}}Types';

export class {{CLASS_NAME}}ServerCommand extends CommandBase<{{CLASS_NAME}}Params, {{CLASS_NAME}}Result> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('{{COMMAND_NAME}}', context, subpath, commander);
  }

  async execute(params: {{CLASS_NAME}}Params): Promise<{{CLASS_NAME}}Result> {
    console.log('🔧 SERVER: Executing {{COMMAND_NAME}}', params);

    // Validate required parameters
    // NOTE: Commands should THROW errors when validation fails, not catch and return success:false
    // This demonstrates BEST PRACTICE error handling for command templates
    //
    // Example validation for a required parameter:
    // if (!params.yourRequiredParam || params.yourRequiredParam.trim() === '') {
    //   throw new ValidationError(
    //     'yourRequiredParam',
    //     `Missing required parameter 'yourRequiredParam'. ` +
    //     `Use the help tool with '{{COMMAND_NAME}}' or see the {{COMMAND_NAME}} README for usage information.`
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
    return create{{CLASS_NAME}}ResultFromParams(params, {
      success: true,
{{RESULT_FIELD_EXAMPLES}}
    });
  }
}
