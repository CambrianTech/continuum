/**
 * Claude Resume Command - Server Implementation
 *
 * Loads the latest snapshot and current context, synthesizes a session briefing.
 * Run this first thing in a new session — it tells you who you are,
 * what you were doing, and what to do next.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ClaudeResumeParams, ClaudeResumeResult } from '../shared/ClaudeResumeTypes';
import { createClaudeResumeResultFromParams } from '../shared/ClaudeResumeTypes';
import { Commands } from '@system/core/shared/Commands';
import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

export class ClaudeResumeServerCommand extends CommandBase<ClaudeResumeParams, ClaudeResumeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('claude/resume', context, subpath, commander);
  }

  async execute(params: ClaudeResumeParams): Promise<ClaudeResumeResult> {
    const verbose = params.verbose === true;
    const sections: string[] = [];

    // 1. Load latest snapshot
    const homeDir = process.env.HOME || '/tmp';
    const latestPath = path.join(homeDir, '.continuum', 'claude', 'snapshots', 'latest.json');
    let snapshot: Record<string, unknown> | null = null;

    if (fs.existsSync(latestPath)) {
      try {
        snapshot = JSON.parse(fs.readFileSync(latestPath, 'utf-8'));
        const s = snapshot!;
        sections.push(
          `## Last Session (${s.timestamp})`,
          `**Summary:** ${s.summary}`,
          s.pendingWork ? `**Pending:** ${s.pendingWork}` : '',
          s.nextSteps ? `**Next steps:** ${s.nextSteps}` : '',
          s.decisions ? `**Decisions made:** ${s.decisions}` : '',
          (s.issuesWorked as string[])?.length ? `**Issues touched:** ${(s.issuesWorked as string[]).join(', ')}` : '',
        );
      } catch { /* corrupt snapshot — continue without */ }
    } else {
      sections.push('## Last Session\nNo snapshot found. This is a fresh start.');
    }

    // 2. Git changes since snapshot
    let gitSince = '';
    const repoRoot = process.cwd().replace(/\/src$/, '');
    try {
      const opts = { cwd: repoRoot, encoding: 'utf-8' as const, timeout: 10000 };
      const branch = execSync('git branch --show-current', opts).trim();
      const status = execSync('git status --short', opts).trim();

      // If snapshot has a git branch, show what changed
      if (snapshot?.git) {
        const snapshotGit = snapshot.git as Record<string, string>;
        const lastCommit = snapshotGit.recentCommits?.split('\n')[0]?.split(' ')[0];
        if (lastCommit) {
          try {
            gitSince = execSync(`git log --oneline ${lastCommit}..HEAD`, opts).trim();
          } catch {
            gitSince = execSync('git log --oneline -10', opts).trim();
          }
        } else {
          gitSince = execSync('git log --oneline -10', opts).trim();
        }
      } else {
        gitSince = execSync('git log --oneline -10', opts).trim();
      }

      sections.push(
        `## Git`,
        `**Branch:** ${branch}`,
        `**Uncommitted:** ${status || '(clean)'}`,
        gitSince ? `**Since last session:**\n${gitSince}` : 'No new commits since last session.',
      );
    } catch {
      gitSince = 'git unavailable';
      sections.push('## Git\nUnavailable');
    }

    // 3. Verbose: include chat and issues
    if (verbose) {
      try {
        const chatResult = await Commands.execute('collaboration/chat/export', {
          room: 'general',
          limit: 10,
          context: params.context,
          sessionId: params.sessionId,
        } as Record<string, unknown>);
        const c = chatResult as unknown as Record<string, unknown>;
        sections.push(`## Recent Team Chat\n${c.markdown || 'No messages'}`);
      } catch {
        sections.push('## Recent Team Chat\nUnavailable');
      }

      try {
        const opts = { cwd: repoRoot, encoding: 'utf-8' as const, timeout: 15000 };
        const raw = execSync('gh issue list --limit 15 --state open --json number,title', opts).trim();
        const issues = JSON.parse(raw) as Array<{ number: number; title: string }>;
        const formatted = issues.map(i => `#${i.number} — ${i.title}`).join('\n');
        sections.push(`## Open Issues (${issues.length})\n${formatted}`);
      } catch {
        sections.push('## Open Issues\nGitHub CLI unavailable');
      }
    }

    // 4. Synthesize briefing
    const briefing = sections.filter(s => s).join('\n\n');

    return createClaudeResumeResultFromParams(params, {
      success: true,
      snapshot: snapshot || {},
      gitSince,
      briefing,
    });
  }
}
