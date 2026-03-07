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
import { exec, execSync } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

const log = Logger.create('ProjectContextSource', 'rag');

export class ProjectContextSource implements RAGSource {
  readonly name = 'project-context';
  readonly priority = 70;
  readonly defaultBudgetPercent = 5;
  readonly isShared = true;

  /** Cached main repo git check (stable for process lifetime) */
  private static _isMainRepoGit: boolean | null = null;

  /**
   * Cached project context per workspace directory.
   * Git status doesn't change every 3 seconds — 30s cache eliminates
   * 14×5 = 70 synchronous shell calls per RAG cycle.
   * Single-flight coalescing prevents thundering herd on cache miss.
   */
  private static _contextCache: Map<string, { section: RAGSection; cachedAt: number }> = new Map();
  private static _contextInflight: Map<string, Promise<RAGSection>> = new Map();
  private static readonly CONTEXT_CACHE_TTL_MS = 30_000;

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

    // Cache key: workspace dir determines git context (shared repo = all personas share one cache entry)
    const cacheKey = workDir;
    const cached = ProjectContextSource._contextCache.get(cacheKey);
    if (cached && Date.now() - cached.cachedAt < ProjectContextSource.CONTEXT_CACHE_TTL_MS) {
      return { ...cached.section, loadTimeMs: performance.now() - startTime };
    }

    // Single-flight: if another persona is already loading this workspace, piggyback
    const inflight = ProjectContextSource._contextInflight.get(cacheKey);
    if (inflight) {
      const result = await inflight;
      return { ...result, loadTimeMs: performance.now() - startTime };
    }

    // Load and cache
    const initialBranch = wsMeta?.branch ?? '';
    const loadPromise = this.loadFresh(context, allocatedBudget, workDir, repoPath, isPersonalWorkspace, initialBranch, startTime);
    ProjectContextSource._contextInflight.set(cacheKey, loadPromise);
    try {
      const section = await loadPromise;
      ProjectContextSource._contextCache.set(cacheKey, { section, cachedAt: Date.now() });
      return section;
    } finally {
      ProjectContextSource._contextInflight.delete(cacheKey);
    }
  }

  private async loadFresh(
    context: RAGSourceContext,
    allocatedBudget: number,
    workDir: string,
    repoPath: string,
    isPersonalWorkspace: boolean,
    initialBranch: string,
    startTime: number,
  ): Promise<RAGSection> {

    try {
      // Resolve branch — from workspace metadata or live git query
      let branch = initialBranch;
      if (!branch) {
        try {
          const { stdout } = await execAsync('git branch --show-current', {
            cwd: workDir, timeout: 3000,
          });
          branch = stdout.trim();
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
  // Git data extraction (async — never blocks event loop)
  // ────────────────────────────────────────────────────────────

  private async getGitStatus(dir: string): Promise<string> {
    try {
      const { stdout } = await execAsync('git status --short --branch', { cwd: dir, timeout: 5000 });
      return stdout.trim();
    } catch {
      return '';
    }
  }

  private async getGitLog(dir: string, count: number): Promise<string> {
    try {
      const { stdout } = await execAsync(
        `git log --oneline --no-decorate -${count}`,
        { cwd: dir, timeout: 5000 },
      );
      return stdout.trim();
    } catch {
      return '';
    }
  }

  private async getTeamBranches(repoPath: string): Promise<string[]> {
    try {
      const { stdout } = await execAsync(
        'git branch --list "ai/*" --format="%(refname:short)"',
        { cwd: repoPath, timeout: 5000 },
      );
      const output = stdout.trim();
      return output ? output.split('\n') : [];
    } catch {
      return [];
    }
  }

  private async getFileTree(dir: string, maxDepth: number): Promise<string> {
    try {
      const { stdout } = await execAsync(
        `find . -maxdepth ${maxDepth} -not -path './.git*' -not -path '*/node_modules/*' -not -name '.DS_Store' | sort | head -50`,
        { cwd: dir, timeout: 5000 },
      );
      return stdout.trim();
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

    // Check all workspaces concurrently
    const checks = Array.from(allWorkspaces)
      .filter(([, meta]) => meta.repoPath === repoPath && meta.branch !== ownBranch)
      .map(async ([handle, meta]) => {
        try {
          const { stdout } = await execAsync(
            'git diff --name-only --diff-filter=U 2>/dev/null || true',
            { cwd: meta.worktreeDir, timeout: 3000 },
          );
          const conflictOutput = stdout.trim();
          const hasConflicts = conflictOutput.length > 0;
          const personaId = handle.replace('project-', '').replace(/-[^-]+$/, '');
          return {
            branch: meta.branch,
            personaId,
            hasConflicts,
            conflictFiles: hasConflicts ? conflictOutput.split('\n') : [],
          } as TeamMemberStatus;
        } catch {
          return null;
        }
      });

    const results = await Promise.all(checks);
    for (const r of results) {
      if (r) statuses.push(r);
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
