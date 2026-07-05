/**
 * Code Shell Watch Command - Server Implementation
 *
 * Watch a shell execution for new output. Blocks until output is available — no timeout, no polling.
 * Returns classified output lines filtered through sentinel rules. Call in a loop until finished is true.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { CodeShellWatchParams, CodeShellWatchResult } from '../shared/CodeShellWatchTypes';
import { createCodeShellWatchResultFromParams } from '../shared/CodeShellWatchTypes';
import { CodeDaemon } from '@daemons/code-daemon/shared/CodeDaemon';

export class CodeShellWatchServerCommand extends CommandBase<CodeShellWatchParams, CodeShellWatchResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/shell/watch', context, subpath, commander);
  }

  async execute(params: CodeShellWatchParams): Promise<CodeShellWatchResult> {
    if (!params.executionId || params.executionId.trim() === '') {
      throw new ValidationError(
        'executionId',
        `Missing required parameter 'executionId'. Use the help tool with 'Code Shell Watch' or see the code/shell/watch README for usage.`
      );
    }

    if (!params.userId) {
      throw new ValidationError(
        'userId',
        'Shell watch operations require a userId (auto-injected for persona tool calls).'
      );
    }
    const personaId = params.userId;

    const result = await CodeDaemon.shellWatch(personaId, params.executionId);

    return createCodeShellWatchResultFromParams(params, {
      success: true,
      executionId: result.execution_id,
      lines: result.lines,
      finished: result.finished,
      exitCode: result.exit_code,
    });
  }
}
