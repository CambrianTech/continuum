/**
 * Git Issue List Command - Server Implementation
 *
 * Lists GitHub issues using gh CLI.
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import { CommandBase, type ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type { GitIssueListParams, GitIssueListResult, GitIssue } from '../shared/GitIssueListTypes';

const execAsync = promisify(exec);

export class GitIssueListServerCommand extends CommandBase<GitIssueListParams, GitIssueListResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('git/issue/list', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<GitIssueListResult> {
    const listParams = params as GitIssueListParams;

    try {
      // Validate environment
      const validation = await this.validateGitEnvironment();
      if (!validation.valid) {
        return transformPayload(listParams, {
          success: false,
          error: validation.error
        });
      }

      // Build gh CLI command
      const state = listParams.state || 'open';
      const limit = listParams.limit || 30;

      let cmd = `gh issue list --state ${state} --limit ${limit}`;

      if (listParams.label) {
        cmd += ` --label "${listParams.label}"`;
      }
      if (listParams.assignee) {
        cmd += ` --assignee "${listParams.assignee}"`;
      }

      cmd += ' --json number,title,state,url,labels,assignees,createdAt,updatedAt';

      console.log(`🐙 GitHub: Listing issues (state=${state}, limit=${limit})`);

      const { stdout, stderr } = await execAsync(cmd, {
        cwd: process.cwd(),
        timeout: 30000
      });

      if (stderr) {
        console.warn(`⚠️  GitHub CLI warning: ${stderr}`);
      }

      const rawIssues = JSON.parse(stdout);

      const issues: GitIssue[] = rawIssues.map((issue: any) => ({
        number: issue.number,
        title: issue.title,
        state: issue.state,
        url: issue.url,
        labels: issue.labels?.map((l: any) => l.name) || [],
        assignee: issue.assignees?.[0]?.login || null,
        createdAt: issue.createdAt,
        updatedAt: issue.updatedAt
      }));

      console.log(`✅ GitHub: Found ${issues.length} issue(s)`);

      return transformPayload(listParams, {
        success: true,
        issues,
        count: issues.length
      });
    } catch (error) {
      console.error(`❌ GitHub: Failed to list issues`);
      console.error(`   Error: ${error instanceof Error ? error.message : String(error)}`);

      return transformPayload(listParams, {
        success: false,
        error: `Failed to list issues: ${error instanceof Error ? error.message : String(error)}`
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
}
