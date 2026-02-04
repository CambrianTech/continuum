/**
 * Git Workspace Init Command - Server Implementation
 *
 * Initialize git workspace for persona collaboration with isolated worktree
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { GitWorkspaceInitParams, GitWorkspaceInitResult } from '../shared/GitWorkspaceInitTypes';
import { createGitWorkspaceInitResultFromParams } from '../shared/GitWorkspaceInitTypes';
import { generateUUID, toShortId, type UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '@system/core/shared/Commands';
import type { DataReadParams, DataReadResult } from '@commands/data/read/shared/DataReadTypes';
import type { UserEntity } from '@system/data/entities/UserEntity';
import type { DataRecord } from '@daemons/data-daemon/shared/DataStorageAdapter';
import { COLLECTIONS } from '@system/data/config/DatabaseConfig';
import * as path from 'path';
import * as fs from 'fs';
import { promisify } from 'util';
import { exec } from 'child_process';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';

import { DataRead } from '../../../../../data/read/shared/DataReadTypes';
const execAsync = promisify(exec);

export class GitWorkspaceInitServerCommand extends CommandBase<GitWorkspaceInitParams, GitWorkspaceInitResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Git Workspace Init', context, subpath, commander);
  }

  async execute(params: GitWorkspaceInitParams): Promise<GitWorkspaceInitResult> {
    console.log('🔧 SERVER: Executing Git Workspace Init', params);

    try {
      // 1. Determine persona ID (from params or auto-injected userId)
      const personaId = params.personaId || params.userId;
      if (!personaId) {
        throw new ValidationError(
          'personaId',
          'Could not determine persona ID from params. ' +
          'Use the help tool with "Git Workspace Init" or see the Git Workspace Init README for usage information.'
        );
      }

      // 2. Get persona user entity to get display name and email
      const userResult = await DataRead.execute<UserEntity>({
        collection: COLLECTIONS.USERS,
        id: personaId
      });

      if (!userResult.success || !userResult.data) {
        throw new Error(`Failed to load user entity for persona ${personaId}`);
      }

      const persona = userResult.data as UserEntity;

      // 3. Generate workspace path (.continuum/sessions/user/shared/{personaId}/workspace)
      const workspacePath = path.resolve(
        process.cwd(),
        '.continuum/sessions/user/shared',
        personaId,
        'workspace'
      );

      // 4. Check if workspace already exists
      if (fs.existsSync(workspacePath)) {
        throw new Error(
          `Workspace already exists at ${workspacePath}. ` +
          'Use git/workspace/clean to remove existing workspace first.'
        );
      }

      // 5. Generate branch name (default: {persona-shortName}/{timestamp})
      const branchName = params.branch || `${persona.shortName}/${Date.now()}`;

      // 6. Get main repo path (current working directory)
      const mainRepoPath = process.cwd();

      // 7. Create worktree directory parent if needed
      const workspaceParent = path.dirname(workspacePath);
      if (!fs.existsSync(workspaceParent)) {
        fs.mkdirSync(workspaceParent, { recursive: true });
      }

      // 8. Validate and coerce paths parameter (REQUIRED)
      // LLMs frequently pass arrays as JSON strings: "[\"src/\"]" instead of ["src/"]
      let paths: string[];
      const rawPaths = params.paths as unknown;
      if (Array.isArray(rawPaths)) {
        paths = rawPaths;
      } else if (typeof rawPaths === 'string') {
        try {
          const parsed = JSON.parse(rawPaths);
          paths = Array.isArray(parsed) ? parsed : [rawPaths];
        } catch {
          // Treat as single path
          paths = [rawPaths];
        }
      } else {
        paths = [];
      }

      if (paths.length === 0) {
        throw new ValidationError(
          'paths',
          'Must specify paths to checkout (e.g., --paths=\'["docs/"]\').\n' +
          'Sparse checkout is required to keep workspaces minimal.\n' +
          'Use the help tool with "Git Workspace Init" for more information.'
        );
      }

      // 9. Create sparse checkout worktree
      console.log(`Creating sparse worktree at ${workspacePath} with paths: ${paths.join(', ')}`);
      await execAsync(
        `git worktree add --no-checkout -b "${branchName}" "${workspacePath}" main`,
        { cwd: mainRepoPath }
      );

      // 10. Enable sparse checkout with specified paths
      const pathsArg = paths.join(' ');
      await execAsync(
        `git sparse-checkout init --cone && git sparse-checkout set ${pathsArg}`,
        { cwd: workspacePath }
      );

      // 11. Checkout the sparse files
      await execAsync(
        `git checkout ${branchName}`,
        { cwd: workspacePath }
      );

      // 12. Set local git config with persona identity
      console.log(`Setting git identity: ${persona.displayName} <${persona.email}>`);
      await execAsync(
        `git config --local user.name "${persona.displayName}"`,
        { cwd: workspacePath }
      );
      await execAsync(
        `git config --local user.email "${persona.email}"`,
        { cwd: workspacePath }
      );

      // 13. Generate workspace ID and short ID
      const workspaceId = generateUUID();
      const shortId = '#' + toShortId(workspaceId);

      console.log(`✅ Workspace created: ${shortId} at ${workspacePath}`);

      // Return successful result
      return createGitWorkspaceInitResultFromParams(params, {
        success: true,
        workspaceId,
        shortId,
        workspacePath,
        branch: branchName
      });

    } catch (error: any) {
      console.error('❌ Git Workspace Init failed:', error);
      return createGitWorkspaceInitResultFromParams(params, {
        success: false,
        error: error.message || 'Failed to initialize git workspace',
        workspaceId: '',
        shortId: '',
        workspacePath: '',
        branch: ''
      });
    }
  }
}
