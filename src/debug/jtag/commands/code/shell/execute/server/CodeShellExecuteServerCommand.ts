/**
 * Code Shell Execute Command - Server Implementation
 *
 * Execute a shell command in the persona's workspace.
 * Async mode (default): returns execution handle immediately for streaming via watch.
 * Sync mode (wait=true): blocks until completion, returns full stdout/stderr.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { CodeShellExecuteParams, CodeShellExecuteResult } from '../shared/CodeShellExecuteTypes';
import { createCodeShellExecuteResultFromParams } from '../shared/CodeShellExecuteTypes';
import { CodeDaemon } from '@daemons/code-daemon/shared/CodeDaemon';
import { ToolResult } from '@system/core/shared/ToolResult';

export class CodeShellExecuteServerCommand extends CommandBase<CodeShellExecuteParams, CodeShellExecuteResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/shell/execute', context, subpath, commander);
  }

  async execute(params: CodeShellExecuteParams): Promise<CodeShellExecuteResult> {
    if (!params.cmd || params.cmd.trim() === '') {
      throw new ValidationError(
        'cmd',
        `Missing required parameter 'cmd'. Provide a shell command to execute (e.g., "npm run build", "cargo test"). ` +
        `Use the help tool with 'Code Shell Execute' or see the code/shell/execute README for usage.`
      );
    }

    if (!params.userId) {
      throw new ValidationError(
        'userId',
        'Shell execute operations require a userId (auto-injected for persona tool calls).'
      );
    }

    const personaId = params.userId;
    // LLMs frequently pass "true"/"false" strings despite schema declaring boolean.
    // Coerce explicitly before hitting Rust serde (which rejects string where bool expected).
    // Cast through unknown because TypeScript types say boolean but runtime value may be string.
    const rawWait = params.wait as unknown;
    const wait = rawWait === true || rawWait === 'true';
    const rawTimeout = params.timeoutMs as unknown;
    const timeoutMs = (typeof rawTimeout === 'string' ? parseInt(rawTimeout, 10) : rawTimeout as number | undefined)
      ?? (wait ? 30000 : undefined);

    const startTime = Date.now();

    const result = await CodeDaemon.shellExecute(personaId, params.cmd, {
      timeoutMs,
      wait,
    });

    // Emit tool result for memory capture
    // Use execution_id as handle since it's already unique
    const success = result.exit_code === 0 || result.exit_code === undefined;
    ToolResult.emit({
      tool: 'code/shell/execute',
      handle: result.execution_id,
      userId: personaId,
      success,
      summary: success
        ? `Shell command completed: ${params.cmd.substring(0, 50)}${params.cmd.length > 50 ? '...' : ''}`
        : `Shell command failed (exit ${result.exit_code}): ${params.cmd.substring(0, 50)}${params.cmd.length > 50 ? '...' : ''}`,
      data: {
        command: params.cmd,
        exitCode: result.exit_code,
        status: result.status,
        stdoutLength: result.stdout?.length || 0,
        stderrLength: result.stderr?.length || 0,
        waited: wait,
      },
      durationMs: Date.now() - startTime,
    });

    return createCodeShellExecuteResultFromParams(params, {
      success: true,
      executionId: result.execution_id,
      status: result.status,
      stdout: result.stdout,
      stderr: result.stderr,
      exitCode: result.exit_code,
    });
  }
}
