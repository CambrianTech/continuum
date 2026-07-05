/**
 * Code Git Command - Server Implementation
 *
 * Routes git operations through the Rust IPC backend for workspace isolation.
 * All operations are scoped to the persona's registered workspace.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { CodeGitParams, CodeGitResult, GitOperation } from '../shared/CodeGitTypes';
import { createCodeGitResultFromParams } from '../shared/CodeGitTypes';
import { CodeDaemon } from '@daemons/code-daemon/shared/CodeDaemon';

const VALID_OPERATIONS = new Set<string>(['status', 'diff', 'log', 'add', 'commit', 'push']);

export class CodeGitServerCommand extends CommandBase<CodeGitParams, CodeGitResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('code/git', context, subpath, commander);
  }

  async execute(params: CodeGitParams): Promise<CodeGitResult> {
    // Validate required parameters
    if (!params.userId) {
      throw new ValidationError(
        'userId',
        'Missing required parameter userId (persona/workspace handle).'
      );
    }

    if (!params.operation || !VALID_OPERATIONS.has(params.operation)) {
      throw new ValidationError(
        'operation',
        `Invalid operation '${params.operation ?? ''}'. Must be one of: ${Array.from(VALID_OPERATIONS).join(', ')}`
      );
    }

    const operation = params.operation as GitOperation;

    switch (operation) {
      case 'status':
        return this.handleStatus(params);
      case 'diff':
        return this.handleDiff(params);
      case 'log':
        return this.handleLog(params);
      case 'add':
        return this.handleAdd(params);
      case 'commit':
        return this.handleCommit(params);
      case 'push':
        return this.handlePush(params);
    }
  }

  private async handleStatus(params: CodeGitParams): Promise<CodeGitResult> {
    const statusInfo = await CodeDaemon.workspaceGitStatus(params.userId);

    const totalChanges = statusInfo.modified.length + statusInfo.added.length
      + statusInfo.deleted.length + statusInfo.untracked.length;

    return createCodeGitResultFromParams(params, {
      success: statusInfo.success,
      operation: 'status',
      status: {
        branch: statusInfo.branch,
        modified: statusInfo.modified,
        added: statusInfo.added,
        deleted: statusInfo.deleted,
        untracked: statusInfo.untracked,
      },
      summary: statusInfo.success
        ? `Branch: ${statusInfo.branch ?? 'unknown'}, ${totalChanges} changed file(s)`
        : `Git status failed: ${statusInfo.error ?? 'unknown error'}`,
    });
  }

  private async handleDiff(params: CodeGitParams): Promise<CodeGitResult> {
    const result = await CodeDaemon.workspaceGitDiff(params.userId, params.staged ?? false);

    return createCodeGitResultFromParams(params, {
      success: result.success,
      operation: 'diff',
      diff: result.diff,
      summary: result.diff
        ? `${result.diff.split('\n').length} lines of diff output`
        : 'No changes',
    });
  }

  private async handleLog(params: CodeGitParams): Promise<CodeGitResult> {
    const result = await CodeDaemon.workspaceGitLog(params.userId, params.count ?? 10);

    return createCodeGitResultFromParams(params, {
      success: result.success,
      operation: 'log',
      log: result.log,
      summary: result.log
        ? `${result.log.trim().split('\n').length} commit(s)`
        : 'No commits',
    });
  }

  private async handleAdd(params: CodeGitParams): Promise<CodeGitResult> {
    if (!params.paths || params.paths.length === 0) {
      throw new ValidationError(
        'paths',
        'The add operation requires at least one path. Use ["."] to stage all changes.'
      );
    }

    const result = await CodeDaemon.workspaceGitAdd(params.userId, params.paths);

    return createCodeGitResultFromParams(params, {
      success: true,
      operation: 'add',
      staged: result.staged,
      summary: `Staged ${result.staged.length} path(s)`,
    });
  }

  private async handleCommit(params: CodeGitParams): Promise<CodeGitResult> {
    if (!params.message || params.message.trim() === '') {
      throw new ValidationError(
        'message',
        'The commit operation requires a non-empty message.'
      );
    }

    const result = await CodeDaemon.workspaceGitCommit(params.userId, params.message.trim());

    return createCodeGitResultFromParams(params, {
      success: true,
      operation: 'commit',
      commitHash: result.hash,
      summary: `Committed: ${result.hash.substring(0, 8)}`,
    });
  }

  private async handlePush(params: CodeGitParams): Promise<CodeGitResult> {
    const result = await CodeDaemon.workspaceGitPush(
      params.userId,
      params.remote ?? 'origin',
      params.branch ?? ''
    );

    return createCodeGitResultFromParams(params, {
      success: true,
      operation: 'push',
      pushOutput: result.output,
      summary: `Pushed to ${params.remote ?? 'origin'}${params.branch ? '/' + params.branch : ''}`,
    });
  }
}
