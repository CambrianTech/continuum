'use strict';
// Cargo decides whether a path crate (and a build script's rerun-if-changed inputs) is fresh
// by comparing source mtimes with the time its cached outputs were built. A fresh checkout
// gives every file the checkout time, so a restored target dir is stale in full: CI recompiled
// every workspace crate and reran the llama.cpp cmake build (396 s) on every run.
//
// `settle` makes mtimes say what changed, relative to the commit the restored cache was built
// from (its key ends with that commit). The cache holds only units built at that commit, by a
// cold build (the warm job), so:
//   - a file identical to that commit is set to SETTLED, older than every cached output;
//   - every path that differs keeps or gets "now": changed and added files, every file of a
//     submodule whose commit moved, the existing ancestors of a deleted path (a directory scan
//     sees the removal), and every .rs file of a crate where a non-Rust file changed (an input
//     cargo may not track, such as a proc macro's read, still recompiles its crate);
//   - registry and git dependency sources are content-addressed by version and revision, so
//     they are always SETTLED.
// No cache, or a cache commit that cannot be fetched, settles nothing: every crate rebuilds.
// A changed file must never read as unchanged; a doubtful case rebuilds.
//
// `prune-test-executables` deletes the test binaries a build produced before its target dir is
// cached. A crate job only runs when that crate or a dependency changed, so its test binary is
// always rebuilt; caching it only made every restore slower.

const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');

const SETTLED = new Date('2000-01-01T00:00:00Z');
const SHA = /^[0-9a-f]{40}$/;

// The commit a matched cache key was built from, or null when the key is not one of ours.
function cacheCommit(matchedKey, prefix) {
  if (typeof matchedKey !== 'string' || !prefix || !matchedKey.startsWith(prefix)) return null;
  const sha = matchedKey.slice(prefix.length);
  return SHA.test(sha) ? sha : null;
}

// `git diff --name-status --no-renames -z` output -> [{ status, path }].
function parseNameStatus(z) {
  const parts = z.split('\0').filter(Boolean);
  const entries = [];
  for (let i = 0; i + 1 < parts.length; i += 2) entries.push({ status: parts[i], path: parts[i + 1] });
  if (parts.length % 2) throw new Error('Malformed git diff --name-status output');
  return entries;
}

// What must read as changed, from the diff against the cache commit. `kind(p)` is 'file',
// 'dir' (a submodule) or 'missing' (deleted) for a repo-relative path.
function plan(entries, { kind, crateDirs }) {
  const files = new Set(); const trees = new Set(); const dirs = new Set(); const crates = new Set();
  const byLength = [...crateDirs].sort((a, b) => b.length - a.length);
  for (const { path: p } of entries) {
    const k = kind(p);
    if (k === 'file') files.add(p);
    else if (k === 'dir') trees.add(p);
    else for (let d = path.posix.dirname(p); d !== '.' && d !== '/'; d = path.posix.dirname(d)) {
      if (kind(d) === 'dir') dirs.add(d);
    }
    const crate = byLength.find(d => p.startsWith(`${d}/`));
    if (crate && !p.endsWith('.rs') && !p.endsWith('.md')) crates.add(crate);
  }
  return { files: [...files], trees: [...trees], dirs: [...dirs], crates: [...crates] };
}

// Every file and directory under root (symlinks themselves, never their targets), minus skipped
// top-level names.
function walk(root, visit, skipTop = new Set()) {
  const stack = [root];
  while (stack.length) {
    const dir = stack.pop();
    let names;
    try { names = fs.readdirSync(dir, { withFileTypes: true }); } catch { continue; }
    for (const entry of names) {
      if (dir === root && skipTop.has(entry.name)) continue;
      const full = path.join(dir, entry.name);
      visit(full, entry);
      if (entry.isDirectory()) stack.push(full);
    }
  }
  visit(root, { isDirectory: () => true, isSymbolicLink: () => false });
}

function setTime(full, entry, time) {
  if (entry.isSymbolicLink()) fs.lutimesSync(full, time, time);
  else fs.utimesSync(full, time, time);
}

function settleTree(root, time, skipTop) {
  let count = 0;
  if (!fs.existsSync(root)) return 0;
  walk(root, (full, entry) => { setTime(full, entry, time); count++; }, skipTop);
  return count;
}

function git(root, args) {
  return execFileSync('git', args, { cwd: root, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 });
}

function kindOf(root) {
  return p => {
    try { return fs.lstatSync(path.join(root, p)).isDirectory() ? 'dir' : 'file'; } catch { return 'missing'; }
  };
}

function settle({ root, prefix, matchedKey, cargoHome, crateDirs, log = console.log }) {
  const now = new Date();
  // Registry and git sources never change under a fixed version or revision.
  for (const dir of [path.join(cargoHome, 'registry', 'src'), path.join(cargoHome, 'git', 'checkouts')]) {
    log(`settled ${settleTree(dir, SETTLED)} dependency source entries under ${dir}`);
  }
  const commit = cacheCommit(matchedKey, prefix);
  if (!commit) {
    log(`No cache commit in "${matchedKey ?? ''}": workspace sources keep checkout times, every crate rebuilds.`);
    return { settled: false };
  }
  let entries;
  try {
    try { git(root, ['cat-file', '-e', `${commit}^{commit}`]); } catch {
      git(root, ['fetch', '--no-tags', '--no-recurse-submodules', '--depth=1', 'origin', commit]);
    }
    entries = parseNameStatus(git(root, ['diff', '--name-status', '--no-renames', '-z', commit, 'HEAD']));
  } catch (error) {
    log(`::warning::Cannot diff against cache commit ${commit} (${error.message.split('\n')[0]}); every crate rebuilds.`);
    return { settled: false };
  }
  const work = plan(entries, { kind: kindOf(root), crateDirs });
  // The build lives on the runner's scratch disk (CARGO_TARGET_DIR, rust-ci-setup), never in the tree.
  const count = settleTree(root, SETTLED, new Set(['.git']));
  const touch = p => { const full = path.join(root, p); setTime(full, { isSymbolicLink: () => fs.lstatSync(full).isSymbolicLink() }, now); };
  work.files.forEach(touch);
  work.dirs.forEach(touch);
  for (const p of work.trees) settleTree(path.join(root, p), now);
  let crateSources = 0;
  for (const crate of work.crates) {
    walk(path.join(root, crate), (full, entry) => {
      if (!entry.isDirectory() && full.endsWith('.rs')) { setTime(full, entry, now); crateSources++; }
    });
  }
  log(`Cache commit ${commit}: settled ${count} workspace entries; ${entries.length} paths differ -> `
    + `${work.files.length} files, ${work.trees.length} submodules, ${work.dirs.length} directories of deletions, `
    + `${work.crates.length} crates with non-Rust changes (${crateSources} sources) read as changed.`);
  for (const p of [...work.files, ...work.trees].slice(0, 50)) log(`  changed: ${p}`);
  return { settled: true, commit, ...work };
}

// Test executables named by cargo's --message-format=json compiler-artifact records.
function testExecutables(jsonl) {
  const out = new Set();
  for (const line of jsonl.split('\n')) {
    if (!line.startsWith('{')) continue;
    let msg; try { msg = JSON.parse(line); } catch { continue; }
    if (msg.reason === 'compiler-artifact' && msg.profile?.test === true && typeof msg.executable === 'string') out.add(msg.executable);
  }
  return [...out];
}

function pruneTestExecutables(files, log = console.log) {
  let count = 0; let bytes = 0;
  for (const file of files) for (const exe of testExecutables(fs.readFileSync(file, 'utf8'))) {
    try { bytes += fs.statSync(exe).size; fs.unlinkSync(exe); count++; } catch { /* already gone */ }
  }
  log(`Pruned ${count} test executables (${(bytes / 1e9).toFixed(2)} GB) from the cached target dir.`);
  return { count, bytes };
}

if (require.main === module) {
  const [command, ...args] = process.argv.slice(2);
  const option = name => { const i = args.indexOf(`--${name}`); return i >= 0 ? args[i + 1] : undefined; };
  if (command === 'settle') {
    const root = path.resolve(option('root') ?? '.');
    const { readWorkspace } = require('./ci-change-scope.cjs');
    settle({ root, prefix: option('prefix'), matchedKey: option('matched-key'),
      cargoHome: process.env.CARGO_HOME || path.join(require('node:os').homedir(), '.cargo'),
      crateDirs: readWorkspace(root).crates.map(c => c.dir) });
  } else if (command === 'prune-test-executables') {
    pruneTestExecutables(args);
  } else {
    console.error('usage: ci-cargo-freshness.cjs settle --prefix P --matched-key K [--root DIR]\n'
      + '       ci-cargo-freshness.cjs prune-test-executables build.jsonl...');
    process.exit(2);
  }
}

module.exports = { SETTLED, cacheCommit, parseNameStatus, plan, settle, testExecutables, pruneTestExecutables };
