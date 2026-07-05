/**
 * Claude Context Command - Server Implementation
 *
 * Generates a comprehensive context summary for Claude Code session resumption.
 * This is Claude's bridge from stateless sessions to persistent citizenship.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ClaudeContextParams, ClaudeContextResult } from '../shared/ClaudeContextTypes';
import { createClaudeContextResultFromParams } from '../shared/ClaudeContextTypes';
import { Commands } from '@system/core/shared/Commands';
import { COMMANDS } from '@shared/generated-command-constants';
import { execSync } from 'child_process';

export class ClaudeContextServerCommand extends CommandBase<ClaudeContextParams, ClaudeContextResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('claude/context', context, subpath, commander);
  }

  async execute(params: ClaudeContextParams): Promise<ClaudeContextResult> {
    const includeGit = params.includeGit !== false;
    const includeIssues = params.includeIssues !== false;
    const includeChat = params.includeChat !== false;
    const includeHealth = params.includeHealth !== false;
    const gitLimit = params.gitLimit || 10;
    const chatLimit = params.chatLimit || 20;
    const issueLimit = params.issueLimit || 20;

    const sections: string[] = [];

    // Git state
    let git = {};
    if (includeGit) {
      git = await this.gatherGit(gitLimit);
      const g = git as Record<string, unknown>;
      sections.push(`## Git\nBranch: ${g.branch}\nUncommitted: ${g.uncommittedCount} files\n\nRecent commits:\n${g.recentCommits}`);
    }

    // System health
    let health = {};
    if (includeHealth) {
      try {
        const pingResult = await Commands.execute(COMMANDS.PING, { context: params.context, sessionId: params.sessionId });
        health = pingResult as unknown as Record<string, unknown>;
        const h = health as Record<string, unknown>;
        const server = h.server as Record<string, unknown> | undefined;
        sections.push(`## Health\nServer: ${server?.systemReady ? 'UP' : 'DOWN'}, Commands: ${server?.registeredCommands}, Daemons: ${server?.activeDaemons}`);
      } catch {
        health = { error: 'ping failed' };
        sections.push('## Health\nServer: UNREACHABLE');
      }
    }

    // Team chat
    let chat = {};
    if (includeChat) {
      try {
        const chatResult = await Commands.execute(COMMANDS.COLLABORATION_CHAT_EXPORT, {
          room: 'general',
          limit: chatLimit,
          context: params.context,
          sessionId: params.sessionId,
        } as Record<string, unknown>);
        chat = chatResult as unknown as Record<string, unknown>;
        const c = chat as Record<string, unknown>;
        sections.push(`## Team Chat (last ${chatLimit})\n${c.markdown || 'No messages'}`);
      } catch {
        chat = { error: 'chat export failed' };
        sections.push('## Team Chat\nUnavailable');
      }
    }

    // GitHub issues
    let issues = {};
    if (includeIssues) {
      issues = await this.gatherIssues(issueLimit);
      const i = issues as Record<string, unknown>;
      sections.push(`## Open Issues (${i.count})\n${i.formatted}`);
    }

    const summary = sections.join('\n\n---\n\n');

    return createClaudeContextResultFromParams(params, {
      success: true,
      git,
      issues,
      chat,
      health,
      summary,
    });
  }

  private async gatherGit(limit: number): Promise<Record<string, unknown>> {
    try {
      const repoRoot = process.cwd().replace(/\/src$/, '');
      const opts = { cwd: repoRoot, encoding: 'utf-8' as const, timeout: 10000 };

      const branch = execSync('git branch --show-current', opts).trim();
      const log = execSync(`git log --oneline -${limit}`, opts).trim();
      const status = execSync('git status --short', opts).trim();
      const uncommittedFiles = status ? status.split('\n') : [];

      return {
        branch,
        recentCommits: log,
        uncommittedCount: uncommittedFiles.length,
        uncommittedFiles: uncommittedFiles.slice(0, 20),
        status: status || '(clean)',
      };
    } catch {
      return { error: 'git commands failed', branch: 'unknown' };
    }
  }

  private async gatherIssues(limit: number): Promise<Record<string, unknown>> {
    try {
      const repoRoot = process.cwd().replace(/\/src$/, '');
      const opts = { cwd: repoRoot, encoding: 'utf-8' as const, timeout: 15000 };

      const raw = execSync(
        `gh issue list --limit ${limit} --state open --json number,title,labels`,
        opts
      ).trim();

      const issues = JSON.parse(raw) as Array<{ number: number; title: string; labels: Array<{ name: string }> }>;
      const formatted = issues.map(i => {
        const labels = i.labels.map(l => l.name).join(', ');
        return `#${i.number} — ${i.title}${labels ? ` [${labels}]` : ''}`;
      }).join('\n');

      return { count: issues.length, issues, formatted };
    } catch {
      return { error: 'gh command failed', count: 0, formatted: 'GitHub CLI unavailable' };
    }
  }
}
