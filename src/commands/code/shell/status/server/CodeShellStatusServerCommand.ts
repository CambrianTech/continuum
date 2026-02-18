/**
 * Code Shell Status Command - Server Implementation
 *
 * Get shell session info for the persona's workspace — cwd, active/total execution count.
 * No parameters required (userId auto-injected by infrastructure).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { CodeShellStatusParams, CodeShellStatusResult } from '../shared/CodeShellStatusTypes';
import { createCodeShellStatusResultFromParams } from '../shared/CodeShellStatusTypes';
import { CodeDaemon } from '@daemons/code-daemon/shared/CodeDaemon';

export class CodeShellStatusServerCommand extends CommandBase<CodeShellStatusParams, CodeShellStatusResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/shell/status', context, subpath, commander);
  }

  async execute(params: CodeShellStatusParams): Promise<CodeShellStatusResult> {
    if (!params.userId) {
      throw new ValidationError(
        'userId',
        'Shell status operations require a userId (auto-injected for persona tool calls).'
      );
    }

    const personaId = params.userId;
    const info = await CodeDaemon.shellStatus(personaId);

    return createCodeShellStatusResultFromParams(params, {
      success: true,
      shellSessionId: info.session_id,
      personaId: info.persona_id,
      cwd: info.cwd,
      workspaceRoot: info.workspace_root,
      activeExecutions: info.active_executions,
      totalExecutions: info.total_executions,
    });
  }
}
