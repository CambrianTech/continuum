/**
 * Code Shell Sentinel Command - Server Implementation
 *
 * Configure sentinel filter rules on a shell execution. Rules classify output lines
 * and control which lines are emitted or suppressed during watch.
 * Patterns are compiled to regex on the Rust side for performance.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { CodeShellSentinelParams, CodeShellSentinelResult } from '../shared/CodeShellSentinelTypes';
import { createCodeShellSentinelResultFromParams } from '../shared/CodeShellSentinelTypes';
import { CodeDaemon } from '@daemons/code-daemon/shared/CodeDaemon';

export class CodeShellSentinelServerCommand extends CommandBase<CodeShellSentinelParams, CodeShellSentinelResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/shell/sentinel', context, subpath, commander);
  }

  async execute(params: CodeShellSentinelParams): Promise<CodeShellSentinelResult> {
    if (!params.executionId || params.executionId.trim() === '') {
      throw new ValidationError(
        'executionId',
        `Missing required parameter 'executionId'. Use the help tool with 'Code Shell Sentinel' or see the code/shell/sentinel README for usage.`
      );
    }

    if (!params.rules || !Array.isArray(params.rules)) {
      throw new ValidationError(
        'rules',
        `Missing required parameter 'rules'. Provide an array of SentinelRule objects. See the code/shell/sentinel README for usage.`
      );
    }

    if (!params.userId) {
      throw new ValidationError(
        'userId',
        'Shell sentinel operations require a userId (auto-injected for persona tool calls).'
      );
    }
    const personaId = params.userId;

    const result = await CodeDaemon.shellSentinel(personaId, params.executionId, params.rules);

    return createCodeShellSentinelResultFromParams(params, {
      success: true,
      applied: result.applied,
      ruleCount: result.ruleCount,
    });
  }
}
