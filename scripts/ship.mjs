#!/usr/bin/env node
// ship.mjs — the per-brick canary PR dance as one command, cross-platform.
//
// Push a feature branch → open a PR into canary → wait for CI → squash-merge →
// delete the branch → sync local canary. Portable (Windows / macOS / Linux): Node
// wrapping the cross-platform git + gh binaries, no bash. The factory runs everywhere
// the product does ([[solve-for-public-users]], [[build-the-factory-as-you-build-the-car]]).
//
// Usage (run while ON a feature branch with commits already made):
//   node scripts/ship.mjs                     # PR title/body filled from your commits
//   node scripts/ship.mjs "feat(x): title"    # explicit title
//   node scripts/ship.mjs "title" "body"      # explicit title + body
//
// Safety: refuses canary/main; requires commits ahead of base; the squash-merge blocks
// on branch protection (required CI), so a red PR CANNOT be merged — fails loud instead.
// Never --no-verify, never --admin. Base defaults to canary (override: SHIP_BASE).

import { execFileSync } from 'node:child_process';

const BASE = process.env.SHIP_BASE || 'canary';

// Run a binary, return trimmed stdout; throws on non-zero (caller decides).
const run = (cmd, args, opts = {}) =>
  execFileSync(cmd, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], ...opts }).trim();
const git = (...a) => run('git', a);
const gh = (...a) => run('gh', a);
const die = (msg) => { console.error(`ship: ${msg}`); process.exit(1); };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const branch = git('rev-parse', '--abbrev-ref', 'HEAD');
if (branch === BASE || branch === 'main') die(`refusing to ship from protected branch '${branch}' — cut a feature branch first.`);

git('fetch', '-q', 'origin', BASE);
const ahead = parseInt(git('rev-list', '--count', `origin/${BASE}..HEAD`), 10);
if (!ahead) die(`no commits ahead of origin/${BASE} — nothing to ship.`);

console.log(`ship: pushing ${branch} (${ahead} commit(s) ahead of ${BASE})…`);
git('push', '-u', 'origin', branch);
try { git('branch', '-f', BASE, `origin/${BASE}`); } catch {} // keep local base honest

const prArgs = ['pr', 'create', '--base', BASE, '--head', branch];
if (process.argv[2]) prArgs.push('--title', process.argv[2], '--body', process.argv[3] || '');
else prArgs.push('--fill');
gh(...prArgs);
console.log(`ship: PR opened → ${gh('pr', 'view', branch, '--json', 'url', '-q', '.url')}`);

// Squash-merge; branch protection enforces required CI so this blocks until green —
// retry while pending, fail loud on any real error.
let merged = false;
for (let i = 1; i <= 18; i++) {
  try {
    execFileSync('gh', ['pr', 'merge', branch, '--squash', '--delete-branch'], { stdio: 'ignore' });
    merged = true;
    break;
  } catch (e) {
    const err = `${e.stderr || ''}${e.stdout || ''}`.toLowerCase();
    if (/not mergeable|required|checks|pending|state|expected|not yet/.test(err)) {
      console.log(`ship: CI not green yet (attempt ${i}/18) — waiting…`);
      await sleep(12000);
      continue;
    }
    die(`merge failed — real error, not pending:\n${e.stderr || e.message}`);
  }
}
if (!merged) die(`gave up waiting for CI. Inspect: gh pr checks ${branch}`);

git('checkout', BASE);
git('pull', '--ff-only');
console.log(`ship: done — ${branch} squash-merged to ${BASE}; local ${BASE} synced.`);
