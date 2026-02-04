/**
 * ProjectContextSource - Injects project workspace context into persona RAG
 *
 * When a persona has an active project workspace (git worktree on any repo),
 * this source surfaces:
 * - Project type and build/test commands
 * - File tree (top 2 levels)
 * - Git branch + status (modified files, ahead/behind)
 * - Recent commits on this branch
 * - Team activity (other ai/* branches on this repo, their status)
 * - Build status (last build result if tracked)
 *
 * This gives personas situational awareness of:
 * - What they're working on (their files, their branch)
 * - What the team is working on (other branches, recent commits)
 * - Who might need help (merge conflicts, build failures)
 *
 * Priority 70 - Between semantic-memory (60) and conversation-history (80).
 * Project context is important for coding activities but shouldn't displace
 * conversation history or identity.
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import { WorkspaceStrategy } from '../../code/server/WorkspaceStrategy';
import { ProjectDetector, type ProjectType } from '../../code/server/ProjectDetector';
import { Logger } from '../../core/logging/Logger';
import { execSync } from 'child_process';

const log = Logger.create('ProjectContextSource', 'rag');

export class ProjectContextSource implements RAGSource {
  readonly name = 'project-context';
  readonly priority = 70;
  readonly defaultBudgetPercent = 12;

  isApplicable(context: RAGSourceContext): boolean {
    // Only include if persona has an active project workspace
    return !!WorkspaceStrategy.getProjectForPersona(context.personaId);
  }

  async load(context: RAGSourceContext, allocatedBudget: number): Promise<RAGSection> {
    const startTime = performance.now();

    const wsMeta = WorkspaceStrategy.getProjectForPersona(context.personaId);
    if (!wsMeta) {
      return this.emptySection(startTime);
    }

    try {
      const gitOpts = { cwd: wsMeta.worktreeDir, stdio: 'pipe' as const, timeout: 5000 };

      // Run git queries concurrently via Promise.all on sync operations
      // These are fast (~5-10ms each) since they're local git operations
      const [projectType, gitStatus, gitLog, teamBranches, fileTree] = await Promise.all([
        ProjectDetector.detect(wsMeta.worktreeDir),
        this.getGitStatus(wsMeta.worktreeDir),
        this.getGitLog(wsMeta.worktreeDir, 5),
        this.getTeamBranches(wsMeta.repoPath),
        this.getFileTree(wsMeta.worktreeDir, 2),
      ]);

      // Check for team members who might need help (merge conflicts)
      const teamStatus = await this.getTeamStatus(wsMeta.repoPath, wsMeta.branch);

      const formatted = this.formatProjectContext({
        projectType,
        branch: wsMeta.branch,
        gitStatus,
        gitLog,
        teamBranches,
        teamStatus,
        fileTree,
        repoPath: wsMeta.repoPath,
      });

      // Respect budget
      const tokenCount = this.estimateTokens(formatted);
      const budgetTokens = Math.floor(allocatedBudget);
      const finalPrompt = tokenCount > budgetTokens
        ? this.formatMinimal(wsMeta.branch, projectType, gitStatus)
        : formatted;

      const finalTokens = this.estimateTokens(finalPrompt);
      const loadTimeMs = performance.now() - startTime;

      log.debug(`Loaded project context (${finalTokens} tokens, ${loadTimeMs.toFixed(1)}ms) for ${context.personaId.slice(0, 8)}`);

      return {
        sourceName: this.name,
        tokenCount: finalTokens,
        loadTimeMs,
        systemPromptSection: finalPrompt,
        metadata: {
          branch: wsMeta.branch,
          repoPath: wsMeta.repoPath,
          projectType: projectType.type,
          teamBranchCount: teamBranches.length,
        },
      };
    } catch (error: any) {
      log.error(`Failed to load project context: ${error.message}`);
      return this.emptySection(startTime, error.message);
    }
  }

  // ────────────────────────────────────────────────────────────
  // Git data extraction (fast, synchronous operations)
  // ────────────────────────────────────────────────────────────

  private async getGitStatus(dir: string): Promise<string> {
    try {
      return execSync('git status --short --branch', { cwd: dir, stdio: 'pipe', timeout: 5000 }).toString().trim();
    } catch {
      return '';
    }
  }

  private async getGitLog(dir: string, count: number): Promise<string> {
    try {
      return execSync(
        `git log --oneline --no-decorate -${count}`,
        { cwd: dir, stdio: 'pipe', timeout: 5000 },
      ).toString().trim();
    } catch {
      return '';
    }
  }

  private async getTeamBranches(repoPath: string): Promise<string[]> {
    try {
      const output = execSync(
        'git branch --list "ai/*" --format="%(refname:short)"',
        { cwd: repoPath, stdio: 'pipe', timeout: 5000 },
      ).toString().trim();
      return output ? output.split('\n') : [];
    } catch {
      return [];
    }
  }

  private async getFileTree(dir: string, maxDepth: number): Promise<string> {
    try {
      // Use find to get a clean tree limited to depth, excluding .git and node_modules
      return execSync(
        `find . -maxdepth ${maxDepth} -not -path './.git*' -not -path '*/node_modules/*' -not -name '.DS_Store' | sort | head -50`,
        { cwd: dir, stdio: 'pipe', timeout: 5000 },
      ).toString().trim();
    } catch {
      return '';
    }
  }

  /**
   * Check team status — detect if anyone has merge conflicts or build failures.
   * This is how smarter AIs know when to help.
   */
  private async getTeamStatus(repoPath: string, ownBranch: string): Promise<TeamMemberStatus[]> {
    const allWorkspaces = WorkspaceStrategy.allProjectWorkspaces;
    const statuses: TeamMemberStatus[] = [];

    for (const [handle, meta] of allWorkspaces) {
      if (meta.repoPath !== repoPath) continue;
      if (meta.branch === ownBranch) continue; // Skip self

      try {
        // Quick check for merge conflicts
        const conflictOutput = execSync(
          'git diff --name-only --diff-filter=U 2>/dev/null || true',
          { cwd: meta.worktreeDir, stdio: 'pipe', timeout: 3000 },
        ).toString().trim();

        const hasConflicts = conflictOutput.length > 0;
        const personaId = handle.replace('project-', '').replace(/-[^-]+$/, '');

        statuses.push({
          branch: meta.branch,
          personaId,
          hasConflicts,
          conflictFiles: hasConflicts ? conflictOutput.split('\n') : [],
        });
      } catch {
        // Skip unreachable workspaces
      }
    }

    return statuses;
  }

  // ────────────────────────────────────────────────────────────
  // Formatting
  // ────────────────────────────────────────────────────────────

  private formatProjectContext(data: {
    projectType: ProjectType;
    branch: string;
    gitStatus: string;
    gitLog: string;
    teamBranches: string[];
    teamStatus: TeamMemberStatus[];
    fileTree: string;
    repoPath: string;
  }): string {
    const sections: string[] = [];

    // Header with project type
    const commands: string[] = [];
    if (data.projectType.buildCommand) commands.push(`Build: ${data.projectType.buildCommand}`);
    if (data.projectType.testCommand) commands.push(`Test: ${data.projectType.testCommand}`);
    if (data.projectType.serveCommand) commands.push(`Serve: ${data.projectType.serveCommand}`);
    sections.push(`## Project Context\nType: ${data.projectType.description}${commands.length ? ' | ' + commands.join(' | ') : ''}`);

    // Your branch status
    if (data.gitStatus) {
      sections.push(`### Your Branch: ${data.branch}\n${data.gitStatus}`);
    }

    // Recent commits
    if (data.gitLog) {
      sections.push(`### Recent Commits\n${data.gitLog}`);
    }

    // File tree (abbreviated)
    if (data.fileTree) {
      sections.push(`### File Tree\n\`\`\`\n${data.fileTree}\n\`\`\``);
    }

    // Team activity
    if (data.teamBranches.length > 0) {
      const teamLines = data.teamBranches
        .filter(b => b !== data.branch) // Exclude own branch
        .map(b => `- ${b}`);
      if (teamLines.length > 0) {
        sections.push(`### Team Branches\n${teamLines.join('\n')}`);
      }
    }

    // Team members needing help
    const needsHelp = data.teamStatus.filter(s => s.hasConflicts);
    if (needsHelp.length > 0) {
      const helpLines = needsHelp.map(s =>
        `- **${s.branch}** has merge conflicts in: ${s.conflictFiles.join(', ')}`
      );
      sections.push(`### Team Needs Help\n${helpLines.join('\n')}\nYou can help by accessing their workspace and resolving conflicts.`);
    }

    return sections.join('\n\n');
  }

  private formatMinimal(branch: string, projectType: ProjectType, gitStatus: string): string {
    return `## Project: ${projectType.description}\nBranch: ${branch}\n${gitStatus}`;
  }

  private emptySection(startTime: number, error?: string): RAGSection {
    return {
      sourceName: this.name,
      tokenCount: 0,
      loadTimeMs: performance.now() - startTime,
      metadata: error ? { error } : { noProject: true },
    };
  }

  private estimateTokens(text: string): number {
    return Math.ceil(text.length / 4);
  }
}

// ────────────────────────────────────────────────────────────
// Types
// ────────────────────────────────────────────────────────────

interface TeamMemberStatus {
  readonly branch: string;
  readonly personaId: string;
  readonly hasConflicts: boolean;
  readonly conflictFiles: string[];
}
