/**
 * {{COMMAND_NAME}} Command - Browser Implementation
 *
 * {{DESCRIPTION}}
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { {{CLASS_NAME}}Params, {{CLASS_NAME}}Result } from '../shared/{{CLASS_NAME}}Types';

export class {{CLASS_NAME}}BrowserCommand extends CommandBase<{{CLASS_NAME}}Params, {{CLASS_NAME}}Result> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('{{COMMAND_NAME}}', context, subpath, commander);
  }

  async execute(params: {{CLASS_NAME}}Params): Promise<{{CLASS_NAME}}Result> {
    console.log('🌐 BROWSER: Delegating {{COMMAND_NAME}} to server');
    return await this.remoteExecute(params);
  }
}
