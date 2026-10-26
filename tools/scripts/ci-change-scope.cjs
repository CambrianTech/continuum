const fs = require('node:fs');
const path = require('node:path');
const { isDeepStrictEqual } = require('node:util');

// The crates this workflow tests, one parallel job each (longest first, so it starts first).
// Which of them run is derived from the workspace graph at selection time, never listed here.
const TESTED_CRATES = ['continuum-core', 'continuum-mind', 'continuum-dreaming', 'continuum-engine', 'continuum-perception', 'continuum-genome',
  'continuum-kernel', 'continuum-live', 'continuum-hands', 'continuum-memory', 'continuum-experience',
  'continuum-cognition', 'continuum-academy', 'continuum-base', 'continuum-host'];
// The cargo features each crate's job builds its tests with: the crate job and the cache-warming
// job both read this, so the cache holds exactly the units the crate jobs ask for. Core is the
// one that compiles and tests the call (livekit-webrtc) and 3D-avatar (avatar-3d) paths a node's
// profile turns on; memory and base have no fixtures feature; the rest test with their fixtures.
const NO_FIXTURES = new Set(['continuum-memory', 'continuum-base']);
const TEST_FEATURES = Object.fromEntries(TESTED_CRATES.map(c => [c,
  c === 'continuum-core' ? 'livekit-webrtc,avatar-3d' : NO_FIXTURES.has(c) ? '' : 'test-fixtures']));

const publisherPath = '.github/workflows/core-binaries.yml';
const publicationGate = "${{ !cancelled() && needs.publication-scope.outputs.publication != 'false' }}";
const schedulingPaths = new Set([
  '.github/workflows/core-binaries-hourly.yml',
  'tools/scripts/core-binaries-hourly.mjs',
  'tools/scripts/tests/core-binaries-hourly.test.mjs',
  'tools/scripts/ci-change-scope.cjs',
  'tools/scripts/tests/ci-change-scope.test.cjs',
]);
const docs = p => (p.startsWith('docs/') || p.endsWith('.md')) && !p.startsWith('docs/genome/');
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);

function publicationConfig(value) {
  if (!object(value) || !object(value.jobs) || !object(value.jobs.build) || !Array.isArray(value.jobs.build.steps)) {
    throw new Error('Missing publication build configuration');
  }
  const copy = structuredClone(value);
  delete copy.on;
  delete copy.jobs['publication-scope'];
  const build = copy.jobs.build;
  // Strip only our exact admission wiring. Unknown conditions/dependencies are protected.
  if (build.needs === 'publication-scope' && build.if === publicationGate) {
    delete build.needs;
    delete build.if;
  }
  return copy;
}

function selectPublication(paths, evidence) {
  if (evidence.eventName !== 'pull_request' || evidence.complete !== true) return true;
  if (paths.some(p => p !== publisherPath && !docs(p) && !schedulingPaths.has(p)
      && p !== 'core/continuum-cli-lifecycle/src/prebuilt_artifact/tests.rs')) return true;
  if (!paths.includes(publisherPath)) return false;
  try {
    return !isDeepStrictEqual(publicationConfig(evidence.publicationBefore), publicationConfig(evidence.publicationAfter));
  } catch {
    return true;
  }
}

// The workspace graph as the checkout declares it: each member's directory, package name and
// path dependencies (normal, dev, build and target-specific), read from the Cargo.toml files.
function readWorkspace(root = '.') {
  const read = file => fs.readFileSync(path.join(root, file), 'utf8');
  const strip = text => text.split('\n').map(line => line.replace(/#.*$/, '')).join('\n');
  const members = strip(read('Cargo.toml')).match(/^\s*members\s*=\s*\[([^\]]*)\]/m);
  if (!members) throw new Error('Cannot read workspace members from Cargo.toml');
  const crates = [...members[1].matchAll(/"([^"]+)"/g)].map(([, dir]) => {
    let section = ''; let name = null; const deps = new Set(); const bins = [];
    for (const line of strip(read(`${dir}/Cargo.toml`)).split('\n')) {
      const header = line.match(/^\s*\[+([^\]]+)\]+/);
      if (header) { section = header[1].trim(); if (section === 'bin') bins.push({}); continue; }
      if (section === 'package' && !name) name = line.match(/^\s*name\s*=\s*"([^"]+)"/)?.[1] ?? null;
      const field = section === 'bin' && line.match(/^\s*(name|path)\s*=\s*"([^"]+)"/);
      if (field) bins[bins.length - 1][field[1]] = field[2];
      const dependency = /(^|\.)(dev-|build-)?dependencies(\.|$)/.test(section);
      for (const [, dep] of dependency ? line.matchAll(/\bpath\s*=\s*"([^"]+)"/g) : []) {
        deps.add(path.posix.normalize(path.posix.join(dir, dep)));
      }
    }
    if (!name) throw new Error(`Cannot read the package name of ${dir}`);
    return { name, dir, deps: [...deps], bins: bins.filter(b => b.name && b.path) };
  });
  return { crates };
}

// Paths whose effect on compilation is not confined to one crate: they select every crate.
const global = p => p === 'Cargo.lock' || p === 'Cargo.toml' || p === 'rust-toolchain.toml'
  || /(^|\/)\.cargo\//.test(p) || /(^|\/)build\.rs$/.test(p) || p.startsWith('core/vendor/');
// The installed-release handoff regressions exercise the CLI binary and its lifecycle crate.
const cliPath = p => p.startsWith('core/continuum-cli-lifecycle/')
  || p === 'core/continuum-core/src/bin/continuum.rs' || p.startsWith('core/continuum-core/src/bin/continuum/');

// Every crate a changed path lies in, plus every crate that depends on one of them.
// Anything outside a workspace member, or a workspace-wide input, selects every crate.
function selectCrates(paths, workspace) {
  const all = { crates: [...TESTED_CRATES], cli: true };
  if (!workspace || !Array.isArray(workspace.crates)) return all;
  const byDir = new Map(workspace.crates.map(c => [c.dir, c]));
  const dirs = [...byDir.keys()].sort((a, b) => b.length - a.length);
  const changed = new Set();
  for (const p of paths) {
    if (global(p)) return all;
    const dir = dirs.find(d => p.startsWith(`${d}/`));
    if (!dir) return all;
    changed.add(dir);
  }
  const dependents = new Map();
  for (const c of workspace.crates) for (const dep of c.deps) {
    if (!dependents.has(dep)) dependents.set(dep, []);
    dependents.get(dep).push(c.dir);
  }
  const affected = new Set(changed);
  for (const dir of affected) for (const next of dependents.get(dir) ?? []) affected.add(next);
  const names = new Set([...affected].map(dir => byDir.get(dir).name));
  // The handoff job also runs when the lifecycle crate is reached through the graph (a floor
  // crate it builds on changed), not only when a CLI path changed.
  return { crates: TESTED_CRATES.filter(name => names.has(name)), cli: paths.some(cliPath) || names.has('continuum-cli-lifecycle') };
}

// The declared binaries (`[[bin]]`) of tested crates whose own source a change touches: the bin's
// file, and for `src/bin/<x>.rs` its module directory `src/bin/<x>/`; a change to the crate's
// Cargo.toml, where the declarations live, selects all of them. Implicit (undeclared) bins are not
// read. Lib and lib-test builds never
// compile a bin, so without this a PR editing `src/main.rs` reached canary uncompiled (card
// 67bf5d81). The CLI binary is left to the handoff job, which already compiles it. A change to the
// lib a bin uses is not covered here; the hourly core-binaries publication compiles every bin.
function selectBins(paths, workspace, crates) {
  const bins = {};
  for (const c of workspace?.crates ?? []) {
    if (!crates.includes(c.name)) continue;
    const touched = (c.bins ?? []).filter(b => {
      const file = path.posix.join(c.dir, b.path);
      const moduleDir = file.endsWith('.rs') && path.posix.basename(path.posix.dirname(file)) === 'bin' ? `${file.slice(0, -3)}/` : null;
      return paths.some(p => p === file || p === `${c.dir}/Cargo.toml` || (moduleDir && p.startsWith(moduleDir)));
    }).map(b => b.name).filter(name => !(c.name === 'continuum-core' && name === 'continuum'));
    if (touched.length) bins[c.name] = touched;
  }
  return bins;
}

// Only explicitly covered non-Rust surfaces may skip Rust compilation.
// Dependency, build, workflow, generated and unknown changes run the full gates.
// `evidence.workspace` (readWorkspace) narrows Rust to the affected crates on pull requests;
// without it, and on pushes to canary/main (the post-merge net), every crate runs.
function selectChecks(files, evidence = {}) {
  const full = { rust: true, installer: true, publication: true, crates: [...TESTED_CRATES], cli: true, bins: {} };
  if (!Array.isArray(files) || !files.length) return full;
  if (files.some(f => !f || typeof f.filename !== 'string' || !f.filename
      || (f.previous_filename !== undefined && (typeof f.previous_filename !== 'string' || !f.previous_filename)))) return full;
  const paths = files.flatMap(f => [f.filename, f.previous_filename].filter(Boolean));
  const windows = p => /^(install\.ps1|tools\/scripts\/(lib\/(windows-(service|prepared|engine-receipt|elevation|media-reconciliation)|install-common|win-modules|win-engine)\.ps1|tests\/windows-(service|process|airc-download|xz)\.test\.ps1|sync-windows-bootstrap\.ps1|register-core-service\.ps1|run-service-hidden\.ps1))$/.test(p);
  const rustPaths = paths.filter(p => !docs(p) && !windows(p));
  const rust = rustPaths.length > 0;
  const workspace = evidence.eventName === 'push' ? null : evidence.workspace;
  const scope = !rust ? { crates: [], cli: false } : selectCrates(rustPaths, workspace);
  const bins = selectBins(rustPaths, workspace, scope.crates);
  return { rust, installer: rust || paths.some(windows), publication: selectPublication(paths, evidence), ...scope, bins };
}
module.exports = { selectChecks, readWorkspace, TESTED_CRATES, TEST_FEATURES };
