/**
 * Git Commit Command - Server Implementation
 *
 * Commit changes in git workspace with persona identity
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { GitCommitParams, GitCommitResult } from '../shared/GitCommitTypes';
import { createGitCommitResultFromParams } from '../shared/GitCommitTypes';
import * as path from 'path';
import * as fs from 'fs';
import { promisify } from 'util';
import { execFile } from 'child_process';
import { SystemPaths } from '@system/core/config/SystemPaths';

const execFileAsync = promisify(execFile);

export class GitCommitServerCommand extends CommandBase<GitCommitParams, GitCommitResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Git Commit', context, subpath, commander);
  }

  async execute(params: GitCommitParams): Promise<GitCommitResult> {
    console.log('🔧 SERVER: Executing Git Commit', params);

    try {
      // 1. Validate message parameter
      if (!params.message || params.message.trim() === '') {
        throw new ValidationError(
          'message',
          'Commit message is required. Use --message="Your commit message".\n' +
          'Use the help tool with "Git Commit" or see the Git Commit README for usage information.'
        );
      }

      // 2. Determine workspace path
      const userId = params.userId || 'unknown';
      const workspacePath = params.workspacePath || path.join(
        SystemPaths.sessions.user,
        'shared',
        userId,
        'workspace'
      );

      // 3. Verify workspace exists
      if (!fs.existsSync(workspacePath)) {
        throw new Error(
          `Workspace not found at ${workspacePath}. ` +
          'Run git/workspace/init first to create a workspace.'
        );
      }

      // 4. Stage files (specific files or all changes)
      if (params.files && params.files.length > 0) {
        await execFileAsync('git', ['add', '--', ...params.files], { cwd: workspacePath });
      } else {
        await execFileAsync('git', ['add', '-A'], { cwd: workspacePath });
      }

      // 5. Commit through normal git hooks. Validation failures must surface
      // to the caller; AI commits do not get a bypass lane.
      await execFileAsync(
        'git',
        ['commit', '-m', params.message],
        { cwd: workspacePath }
      );

      // 6. Get commit hash
      const { stdout: commitHash } = await execFileAsync(
        'git',
        ['rev-parse', 'HEAD'],
        { cwd: workspacePath }
      );
      const fullHash = String(commitHash).trim();
      const shortHash = fullHash.substring(0, 7);

      // 7. Count files committed
      const { stdout: filesOutput } = await execFileAsync(
        'git',
        ['diff-tree', '--no-commit-id', '--name-only', '-r', 'HEAD'],
        { cwd: workspacePath }
      );
      const filesCommitted = String(filesOutput).trim().split('\n').filter(f => f).length;

      console.log(`✅ Committed ${filesCommitted} files: ${shortHash}`);

      return createGitCommitResultFromParams(params, {
        success: true,
        commitHash: fullHash,
        shortHash,
        filesCommitted
      });

    } catch (error: unknown) {
      console.error('❌ Git commit failed:', error);
      const message = error instanceof Error ? error.message : String(error);
      return createGitCommitResultFromParams(params, {
        success: false,
        error: new ValidationError('git commit', message || 'Failed to commit changes', { cause: error }),
        commitHash: '',
        shortHash: '',
        filesCommitted: 0
      });
    }
  }
}
