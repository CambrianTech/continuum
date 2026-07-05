/**
 * Code Shell Kill Command - Server Implementation
 *
 * Kill a running shell execution. Uses the executionId returned by code/shell/execute.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { CodeShellKillParams, CodeShellKillResult } from '../shared/CodeShellKillTypes';
import { createCodeShellKillResultFromParams } from '../shared/CodeShellKillTypes';
import { CodeDaemon } from '@daemons/code-daemon/shared/CodeDaemon';

export class CodeShellKillServerCommand extends CommandBase<CodeShellKillParams, CodeShellKillResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/shell/kill', context, subpath, commander);
  }

  async execute(params: CodeShellKillParams): Promise<CodeShellKillResult> {
    if (!params.executionId || params.executionId.trim() === '') {
      throw new ValidationError(
        'executionId',
        `Missing required parameter 'executionId'. Provide the execution handle from code/shell/execute. ` +
        `Use the help tool with 'Code Shell Kill' or see the code/shell/kill README for usage.`
      );
    }

    if (!params.userId) {
      throw new ValidationError(
        'userId',
        'Shell kill operations require a userId (auto-injected for persona tool calls).'
      );
    }

    const personaId = params.userId;
    await CodeDaemon.shellKill(personaId, params.executionId);

    return createCodeShellKillResultFromParams(params, {
      success: true,
      executionId: params.executionId,
      killed: true,
    });
  }
}
