/**
 * ProjectContextSource - Injects project workspace context into persona RAG
 *
 * Provides two modes of project awareness:
 *
 * 1. Personal workspace — when a persona has their own git worktree (via code/* tools),
 *    this shows THEIR branch, THEIR changes, and team activity on the same repo.
 *
 * 2. Shared repository fallback — when no personal workspace exists yet, this shows
 *    the main repository context (branch, status, recent commits, file tree). This
 *    ensures personas ALWAYS have codebase awareness, even before invoking code/* tools.
 *
 * Context includes:
 * - Project type and build/test commands
 * - File tree (top 2 levels)
 * - Git branch + status (modified files, ahead/behind)
 * - Recent commits on this branch
 * - Team activity (other ai/* branches on this repo, their status)
 *
 * Priority 70 - Between semantic-memory (60) and conversation-history (80).
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

  /** Cached main repo git check (stable for process lifetime) */
  private static _isMainRepoGit: boolean | null = null;

  isApplicable(context: RAGSourceContext): boolean {
    // If persona has their own project workspace, always applicable
    if (WorkspaceStrategy.getProjectForPersona(context.personaId)) {
      return true;
    }
    // Fall back: provide main repo context so personas always see the codebase
    return ProjectContextSource.isMainRepoGit();
  }

  async load(context: RAGSourceContext, allocatedBudget: number): Promise<RAGSection> {
    const startTime = performance.now();

    const wsMeta = WorkspaceStrategy.getProjectForPersona(context.personaId);

    // Resolve workspace directory — personal worktree or main repo
    const workDir = wsMeta?.worktreeDir ?? process.cwd();
    const repoPath = wsMeta?.repoPath ?? process.cwd();
    const isPersonalWorkspace = !!wsMeta;

    try {
      // Resolve branch — from workspace metadata or live git query
      let branch = wsMeta?.branch ?? '';
      if (!branch) {
        try {
          branch = execSync('git branch --show-current', {
            cwd: workDir, stdio: 'pipe', timeout: 3000,
          }).toString().trim();
        } catch {
          branch = 'unknown';
        }
      }

      // Run git queries concurrently (all fast, local git operations)
      const [projectType, gitStatus, gitLog, teamBranches, fileTree] = await Promise.all([
        ProjectDetector.detect(workDir),
        this.getGitStatus(workDir),
        this.getGitLog(workDir, 5),
        this.getTeamBranches(repoPath),
        this.getFileTree(workDir, 2),
      ]);

      // Team status only makes sense when persona has their own workspace
      // (can't detect merge conflicts in someone else's worktree from main repo)
      const teamStatus = isPersonalWorkspace
        ? await this.getTeamStatus(repoPath, branch)
        : [];

      const formatted = this.formatProjectContext({
        projectType,
        branch,
        gitStatus,
        gitLog,
        teamBranches,
        teamStatus,
        fileTree,
        repoPath,
        isPersonalWorkspace,
      });

      // Respect budget
      const tokenCount = this.estimateTokens(formatted);
      const budgetTokens = Math.floor(allocatedBudget);
      const finalPrompt = tokenCount > budgetTokens
        ? this.formatMinimal(branch, projectType, gitStatus, isPersonalWorkspace)
        : formatted;

      const finalTokens = this.estimateTokens(finalPrompt);
      const loadTimeMs = performance.now() - startTime;

      const mode = isPersonalWorkspace ? 'personal workspace' : 'shared repo';
      log.debug(`Loaded project context [${mode}] (${finalTokens} tokens, ${loadTimeMs.toFixed(1)}ms) for ${context.personaId.slice(0, 8)}`);

      return {
        sourceName: this.name,
        tokenCount: finalTokens,
        loadTimeMs,
        systemPromptSection: finalPrompt,
        metadata: {
          branch,
          repoPath,
          projectType: projectType.type,
          teamBranchCount: teamBranches.length,
          isPersonalWorkspace,
        },
      };
    } catch (error: any) {
      log.error(`Failed to load project context: ${error.message}`);
      return this.emptySection(startTime, error.message);
    }
  }

  /**
   * Check if process.cwd() is inside a git repository.
   * Cached for process lifetime (repo doesn't move at runtime).
   */
  private static isMainRepoGit(): boolean {
    if (ProjectContextSource._isMainRepoGit === null) {
      try {
        execSync('git rev-parse --is-inside-work-tree', {
          cwd: process.cwd(),
          stdio: 'pipe',
          timeout: 3000,
        });
        ProjectContextSource._isMainRepoGit = true;
      } catch {
        ProjectContextSource._isMainRepoGit = false;
      }
    }
    return ProjectContextSource._isMainRepoGit;
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
    isPersonalWorkspace: boolean;
  }): string {
    const sections: string[] = [];

    // Header with project type
    const commands: string[] = [];
    if (data.projectType.buildCommand) commands.push(`Build: ${data.projectType.buildCommand}`);
    if (data.projectType.testCommand) commands.push(`Test: ${data.projectType.testCommand}`);
    if (data.projectType.serveCommand) commands.push(`Serve: ${data.projectType.serveCommand}`);

    const workspaceLabel = data.isPersonalWorkspace
      ? '## Your Workspace'
      : '## Shared Repository';
    sections.push(`${workspaceLabel}\nType: ${data.projectType.description}${commands.length ? ' | ' + commands.join(' | ') : ''}`);

    // Branch status — distinguish personal vs shared
    if (data.gitStatus) {
      const branchLabel = data.isPersonalWorkspace
        ? `### Your Branch: ${data.branch}`
        : `### Current Branch: ${data.branch}`;
      sections.push(`${branchLabel}\n${data.gitStatus}`);
    }

    // Workspace hint for shared mode — guide personas to bootstrap their workspace
    if (!data.isPersonalWorkspace) {
      sections.push(
        `### Workspace Status\n` +
        `You are viewing the shared repository. When you use code/* tools (code/read, code/write, code/shell/execute), ` +
        `a personal workspace with your own git branch will be created automatically.\n` +
        `Your branch will be named ai/{your-name}/work — you can commit freely without affecting the main branch.`
      );
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

  private formatMinimal(branch: string, projectType: ProjectType, gitStatus: string, isPersonalWorkspace: boolean): string {
    const label = isPersonalWorkspace ? 'Your Workspace' : 'Shared Repository';
    return `## ${label}: ${projectType.description}\nBranch: ${branch}\n${gitStatus}`;
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
