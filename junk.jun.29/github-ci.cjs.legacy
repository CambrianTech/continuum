/**
 * GitHub CI Integration for Continuum
 * Allows AIs to check PR status, CI failures, and push fixes
 */

const { exec } = require('child_process');
const { promisify } = require('util');
const fs = require('fs');

const execAsync = promisify(exec);

class GitHubCIIntegration {
  constructor() {
    this.repoPath = process.cwd();
    this.apiBase = 'https://api.github.com';
  }

  async getCurrentPRInfo() {
    try {
      // Get current branch
      const { stdout: branch } = await execAsync('git branch --show-current');
      const currentBranch = branch.trim();
      
      // Get repo info
      const { stdout: remote } = await execAsync('git config --get remote.origin.url');
      const repoUrl = remote.trim();
      
      // Parse owner/repo from URL
      const match = repoUrl.match(/github\.com[/:](.*?)\/(.*)\.git/) || repoUrl.match(/github\.com[/:](.*?)\/(.*)/);
      if (!match) throw new Error('Not a GitHub repository');
      
      const [, owner, repo] = match;
      
      return {
        branch: currentBranch,
        owner: owner,
        repo: repo.replace('.git', ''),
        repoUrl: repoUrl
      };
    } catch (error) {
      throw new Error(`Failed to get PR info: ${error.message}`);
    }
  }

  async checkCIStatus() {
    try {
      const prInfo = await this.getCurrentPRInfo();
      
      // Get latest commit
      const { stdout: commit } = await execAsync('git rev-parse HEAD');
      const sha = commit.trim();
      
      // Use gh CLI to get CI status
      const { stdout: statusOutput } = await execAsync(`gh api repos/${prInfo.owner}/${prInfo.repo}/commits/${sha}/status`);
      const status = JSON.parse(statusOutput);
      
      // Get check runs
      const { stdout: checksOutput } = await execAsync(`gh api repos/${prInfo.owner}/${prInfo.repo}/commits/${sha}/check-runs`);
      const checks = JSON.parse(checksOutput);
      
      return {
        overall_status: status.state, // pending, success, failure
        commit: sha,
        check_runs: checks.check_runs.map(check => ({
          name: check.name,
          status: check.status, // queued, in_progress, completed
          conclusion: check.conclusion, // success, failure, neutral, cancelled, timed_out, action_required
          html_url: check.html_url,
          details_url: check.details_url
        })),
        statuses: status.statuses
      };
    } catch (error) {
      throw new Error(`Failed to check CI status: ${error.message}`);
    }
  }

  async getCILogs(checkName) {
    try {
      const prInfo = await this.getCurrentPRInfo();
      const { stdout: commit } = await execAsync('git rev-parse HEAD');
      const sha = commit.trim();
      
      // Get specific check run details
      const { stdout: checksOutput } = await execAsync(`gh api repos/${prInfo.owner}/${prInfo.repo}/commits/${sha}/check-runs`);
      const checks = JSON.parse(checksOutput);
      
      const targetCheck = checks.check_runs.find(check => 
        check.name.toLowerCase().includes(checkName.toLowerCase())
      );
      
      if (!targetCheck) {
        throw new Error(`Check run '${checkName}' not found`);
      }
      
      // Get check run annotations (errors/warnings)
      const { stdout: annotationsOutput } = await execAsync(`gh api ${targetCheck.url}/annotations`);
      const annotations = JSON.parse(annotationsOutput);
      
      return {
        check_name: targetCheck.name,
        status: targetCheck.status,
        conclusion: targetCheck.conclusion,
        started_at: targetCheck.started_at,
        completed_at: targetCheck.completed_at,
        html_url: targetCheck.html_url,
        annotations: annotations.map(ann => ({
          path: ann.path,
          start_line: ann.start_line,
          end_line: ann.end_line,
          message: ann.message,
          annotation_level: ann.annotation_level, // notice, warning, failure
          raw_details: ann.raw_details
        }))
      };
    } catch (error) {
      throw new Error(`Failed to get CI logs: ${error.message}`);
    }
  }

  async getPRComments() {
    try {
      const prInfo = await this.getCurrentPRInfo();
      
      // Get PR number for current branch
      const { stdout: prOutput } = await execAsync(`gh pr list --head ${prInfo.branch} --json number,title,url`);
      const prs = JSON.parse(prOutput);
      
      if (prs.length === 0) {
        throw new Error(`No PR found for branch ${prInfo.branch}`);
      }
      
      const pr = prs[0];
      
      // Get PR comments
      const { stdout: commentsOutput } = await execAsync(`gh api repos/${prInfo.owner}/${prInfo.repo}/issues/${pr.number}/comments`);
      const comments = JSON.parse(commentsOutput);
      
      return {
        pr_number: pr.number,
        pr_title: pr.title,
        pr_url: pr.url,
        comments: comments.map(comment => ({
          id: comment.id,
          user: comment.user.login,
          body: comment.body,
          created_at: comment.created_at,
          updated_at: comment.updated_at
        }))
      };
    } catch (error) {
      throw new Error(`Failed to get PR comments: ${error.message}`);
    }
  }

  async createCommitAndPush(message, files = []) {
    try {
      // Stage files
      if (files.length > 0) {
        for (const file of files) {
          await execAsync(`git add "${file}"`);
        }
      } else {
        await execAsync('git add .');
      }
      
      // Check if there are changes to commit
      const { stdout: statusOutput } = await execAsync('git status --porcelain');
      if (!statusOutput.trim()) {
        return { message: 'No changes to commit' };
      }
      
      // Commit
      await execAsync(`git commit -m "${message}"`);
      
      // Push
      const { stdout: pushOutput } = await execAsync('git push');
      
      // Get latest commit info
      const { stdout: commitSha } = await execAsync('git rev-parse HEAD');
      
      return {
        success: true,
        commit_sha: commitSha.trim(),
        message: message,
        push_output: pushOutput
      };
    } catch (error) {
      throw new Error(`Failed to commit and push: ${error.message}`);
    }
  }

  async fixCIFailure(failureDetails) {
    try {
      // This method will be called by the AI to attempt fixes
      const prInfo = await this.getCurrentPRInfo();
      
      return {
        repo: prInfo,
        suggested_actions: [
          'Analyze CI failure logs',
          'Identify root cause of failure',
          'Implement appropriate fix',
          'Test fix locally if possible',
          'Commit and push fix'
        ],
        failure_context: failureDetails
      };
    } catch (error) {
      throw new Error(`Failed to analyze CI failure: ${error.message}`);
    }
  }
}

module.exports = GitHubCIIntegration;