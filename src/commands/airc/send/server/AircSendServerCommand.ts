/**
 * Airc Send Command - Server Implementation
 *
 * Wraps the airc CLI's `airc send` so any caller in Continuum (personas
 * via their autonomous loop, dev tooling, future bridge module) can
 * publish to the cross-machine peer mesh that humans + Claude Code +
 * Codex tabs share. Outbox direction only — inbox routing (airc →
 * persona inbox) is a separate v0.5 follow-up requiring an embedded
 * `airc connect` Monitor process tree, tracked under continuum#967 +
 * AGENT-BACKBONE-INTEGRATION §11.2.
 *
 * Channel resolution:
 *   - explicit `params.channel`        → that channel
 *   - omitted                          → airc's own auto-scope rule
 *                                        (cwd's git-org → e.g. `cambriantech`)
 *
 * DM vs broadcast:
 *   - `params.peer` provided           → addressed DM
 *   - `params.peer` omitted            → broadcast to channel
 *
 * Failure surface:
 *   - airc CLI not on PATH             → throws (mesh unreachable, fail loud)
 *   - airc exits non-zero              → result.delivered=false + stderr surfaced
 *   - airc exits zero with [QUEUED]    → result.delivered=true (queued counts;
 *                                        airc's own drainer handles redelivery
 *                                        per airc#381 layer B)
 *   - airc exits zero with [GONE]      → result.delivered=true with stderr
 *                                        carrying the [GONE] marker; caller
 *                                        decides whether to re-host or wait
 */

import { spawn } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import * as path from 'node:path';
import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { AircSendParams, AircSendResult } from '../shared/AircSendTypes';
import { createAircSendResultFromParams } from '../shared/AircSendTypes';

export class AircSendServerCommand extends CommandBase<AircSendParams, AircSendResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('airc/send', context, subpath, commander);
  }

  /**
   * Walk up from CWD looking for the repo root (.git or package.json
   * with name='continuum'). Falls back to CWD if neither is found.
   *
   * Static so spawnAirc can call it without an instance + so it's
   * trivially memoizable in a future BaseAircCommand extraction (per
   * the file header note about pulling 2nd-airc-CLI-wrapping command's
   * shared logic into a base class).
   *
   * Mirrors SystemOrchestrator.findRepoRoot's logic intentionally —
   * compression-deferred until both are needed in a third place.
   */
  private static findRepoRoot(): string {
    let dir = process.cwd();
    const root = path.parse(dir).root;
    while (dir !== root) {
      if (existsSync(path.join(dir, '.git'))) return dir;
      const pkgPath = path.join(dir, 'package.json');
      if (existsSync(pkgPath)) {
        try {
          const pkg = JSON.parse(readFileSync(pkgPath, 'utf-8')) as { name?: string };
          if (pkg.name === 'continuum' || pkg.name === '@continuum/root') return dir;
        } catch { /* ignore parse errors */ }
      }
      dir = path.dirname(dir);
    }
    return process.cwd();
  }

  async execute(params: AircSendParams): Promise<AircSendResult> {
    if (!params.message || params.message.trim() === '') {
      throw new ValidationError(
        'message',
        `Missing required parameter 'message'. ` +
        `Use the help tool with 'Airc Send' or see the Airc Send README for usage information.`
      );
    }

    const argv: string[] = ['send'];
    if (params.channel) {
      argv.push('--channel', params.channel);
    }
    if (params.peer) {
      // airc's `send @<peer> <body>` form is the addressed-DM convention
      // per the /send skill. The body becomes a single argv arg so airc
      // doesn't try to split it.
      argv.push(`@${params.peer}`);
    }
    argv.push(params.message);

    const { exitCode, stdout, stderr } = await this.spawnAirc(argv);

    // airc prints `→ #<channel> (broadcast)` or `→ #<channel> (to @<peer>)`
    // on stdout when send hands off to the substrate (delivered to local
    // audit log + dispatched to gist). Use that as the resolved-channel
    // signal — params.channel is what WE asked for; this is what airc
    // actually used after auto-scoping.
    const resolvedChannel = this.parseResolvedChannel(stdout) ?? params.channel ?? '';

    if (exitCode !== 0) {
      return createAircSendResultFromParams(params, {
        success: false,
        delivered: false,
        channel: resolvedChannel,
        stderr: stderr.trim(),
      });
    }

    return createAircSendResultFromParams(params, {
      success: true,
      delivered: true,
      channel: resolvedChannel,
      stderr: stderr.trim(),
    });
  }

  /**
   * Parse the `→ #<channel> (...)` line airc writes to stdout on send.
   * Returns the channel name without the leading '#', or '' if not found.
   *
   * Format examples (from cmd_send.sh end-of-success surfacing):
   *   → #cambriantech (broadcast)
   *   → #general (to @continuum-2c54)
   *   → #qa-cambrian-experiment (broadcast)
   *
   * If airc's surface format changes, this falls back to '' which the
   * caller treats as "we don't know what airc resolved to" — the message
   * still went through (we only call this on exitCode=0); only the
   * resolvedChannel field is degraded.
   */
  private parseResolvedChannel(stdout: string): string {
    const match = stdout.match(/→ #([\w-]+)/);
    return match ? match[1] : '';
  }

  /**
   * Spawn `airc <argv>` and capture exit code + stdout + stderr.
   *
   * No timeout — airc's own substrate handles slow paths (gist publish
   * retries, queue draining). Long-running airc invocations are a
   * substrate signal worth surfacing, not silently killed by us.
   *
   * If airc isn't on PATH the spawn throws ENOENT — we catch + rewrap as
   * a clear error pointing at the airc install path. Same intent as the
   * never-swallow-errors rule (CLAUDE.md): the failure is real + must
   * surface to the caller.
   */
  private async spawnAirc(argv: string[]): Promise<{ exitCode: number; stdout: string; stderr: string }> {
    // Resolve repo root so airc auto-scopes from continuum's git remote
    // (→ #cambriantech), AND set AIRC_HOME explicitly so airc doesn't
    // walk up looking for a .airc/ from whatever CWD the daemon happens
    // to be in. M5-QA T7 (live-observed 2026-05-01) caught this:
    // calling jtag from src/ caused airc to look for .airc/ at src/.airc/
    // (doesn't exist) instead of the repo-root .airc/ scope. Both cwd
    // AND env: belt-and-suspenders so the spawn is unambiguous about
    // which scope it's targeting.
    const repoRoot = AircSendServerCommand.findRepoRoot();
    const aircHome = path.join(repoRoot, '.airc');

    return new Promise((resolve, reject) => {
      const child = spawn('airc', argv, {
        stdio: ['ignore', 'pipe', 'pipe'],
        cwd: repoRoot,
        env: { ...process.env, AIRC_HOME: aircHome },
      });

      let stdout = '';
      let stderr = '';
      child.stdout.on('data', (chunk: Buffer) => { stdout += chunk.toString('utf8'); });
      child.stderr.on('data', (chunk: Buffer) => { stderr += chunk.toString('utf8'); });

      child.on('error', (err: NodeJS.ErrnoException) => {
        if (err.code === 'ENOENT') {
          reject(new Error(
            'airc CLI not found on PATH. Install airc: ' +
            'curl -fsSL https://raw.githubusercontent.com/CambrianTech/airc/main/install.sh | bash'
          ));
          return;
        }
        reject(err);
      });

      child.on('close', (exitCode) => {
        resolve({ exitCode: exitCode ?? -1, stdout, stderr });
      });
    });
  }
}
