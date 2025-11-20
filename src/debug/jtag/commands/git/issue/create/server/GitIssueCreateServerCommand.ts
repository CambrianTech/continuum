/**
 * Git Issue Create Command - Server Implementation
 *
 * Creates GitHub issues using gh CLI.
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import { CommandBase, type ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type { GitIssueCreateParams, GitIssueCreateResult } from '../shared/GitIssueCreateTypes';
import { Commands } from '../../../../../system/core/shared/Commands';
import type { SessionGetUserParams, SessionGetUserResult } from '../../../../session/get-user/shared/SessionGetUserTypes';

const execAsync = promisify(exec);

export class GitIssueCreateServerCommand extends CommandBase<GitIssueCreateParams, GitIssueCreateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('git/issue/create', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<GitIssueCreateResult> {
    const createParams = params as GitIssueCreateParams;

    try {
      // Validate environment
      const validation = await this.validateGitEnvironment();
      if (!validation.valid) {
        return transformPayload(createParams, {
          success: false,
          error: validation.error
        });
      }

      // Automatically add attribution for PersonaUsers
      let body = createParams.body;

      console.log('🔍 Attribution Debug: sessionId =', createParams.sessionId);

      try {
        // Call session/get-user without targetSessionId - it will use the caller's sessionId automatically
        const userResult = await Commands.execute<SessionGetUserParams, SessionGetUserResult>('session/get-user', {}) as SessionGetUserResult;

        console.log('🔍 Attribution Debug: session/get-user result =', JSON.stringify(userResult, null, 2));

        if (userResult.success && userResult.user) {
          console.log('🔍 Attribution Debug: user found, userType =', userResult.user.userType, 'displayName =', userResult.user.displayName);

          if (userResult.user.userType === 'persona') {
            const displayName = userResult.user.displayName || userResult.user.uniqueId || 'Unknown AI';
            const timestamp = new Date().toISOString();
            body += `\n\n---\n**Created by:** ${displayName} (AI)\n**Timestamp:** ${timestamp}`;
            console.log('✅ Attribution added for PersonaUser:', displayName);
          } else {
            console.log('⏭️  Attribution skipped: userType is not "persona"');
          }
        } else {
          console.log('⚠️  Attribution skipped: user lookup failed or returned no user');
        }
      } catch (error) {
        // If user lookup fails, continue without attribution
        console.warn('❌ Failed to look up user for attribution:', error);
      }

      // Build gh CLI command
      // Handle labels as either string or array (CLI passes string, programmatic usage may pass array)
      const labels = Array.isArray(createParams.labels)
        ? createParams.labels.join(',')
        : createParams.labels || '';
      const assignee = createParams.assignee || '';

      let cmd = `gh issue create --title "${this.escapeShell(createParams.title)}" --body "${this.escapeShell(body)}"`;

      if (labels) {
        cmd += ` --label "${labels}"`;
      }
      if (assignee) {
        cmd += ` --assignee "${assignee}"`;
      }
      if (createParams.milestone) {
        cmd += ` --milestone ${createParams.milestone}`;
      }

      console.log(`🐙 GitHub: Creating issue "${createParams.title}"`);

      // gh issue create returns the issue URL
      const { stdout: issueUrl, stderr } = await execAsync(cmd, {
        cwd: process.cwd(),
        timeout: 30000
      });

      if (stderr) {
        console.warn(`⚠️  GitHub CLI warning: ${stderr}`);
      }

      // Extract issue number from URL (e.g., https://github.com/owner/repo/issues/123)
      const urlMatch = issueUrl.trim().match(/\/issues\/(\d+)$/);
      if (!urlMatch) {
        throw new Error(`Failed to parse issue URL: ${issueUrl}`);
      }

      const issueNumber = parseInt(urlMatch[1], 10);

      // Fetch full issue details with gh issue view
      const { stdout: issueJson } = await execAsync(
        `gh issue view ${issueNumber} --json number,url,title,state,labels`,
        {
          cwd: process.cwd(),
          timeout: 30000
        }
      );

      const issue = JSON.parse(issueJson);

      console.log(`✅ GitHub: Created issue #${issue.number}: ${issue.url}`);

      return transformPayload(createParams, {
        success: true,
        issue: {
          number: issue.number,
          url: issue.url,
          title: issue.title,
          state: issue.state,
          labels: issue.labels?.map((l: any) => l.name) || []
        }
      });
    } catch (error) {
      console.error(`❌ GitHub: Failed to create issue`);
      console.error(`   Error: ${error instanceof Error ? error.message : String(error)}`);

      return transformPayload(createParams, {
        success: false,
        error: `Failed to create issue: ${error instanceof Error ? error.message : String(error)}`
      });
    }
  }

  private async validateGitEnvironment(): Promise<{ valid: boolean; error?: string }> {
    try {
      // Check gh CLI installed
      await execAsync('which gh');
    } catch {
      return {
        valid: false,
        error: 'GitHub CLI (gh) not installed. Install: brew install gh'
      };
    }

    try {
      // Check authentication
      await execAsync('gh auth status');
    } catch {
      return {
        valid: false,
        error: 'Not authenticated with GitHub. Run: gh auth login'
      };
    }

    return { valid: true };
  }

  private escapeShell(str: string): string {
    return str.replace(/"/g, '\\"').replace(/\$/g, '\\$').replace(/`/g, '\\`');
  }
}
