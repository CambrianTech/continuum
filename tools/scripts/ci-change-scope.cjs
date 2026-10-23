const fs = require('node:fs');
const path = require('node:path');
const { isDeepStrictEqual } = require('node:util');

// The crates this workflow tests, one parallel job each (longest first, so it starts first).
// Which of them run is derived from the workspace graph at selection time, never listed here.
const TESTED_CRATES = ['continuum-core', 'continuum-mind', 'continuum-engine', 'continuum-perception', 'continuum-genome',
  'continuum-kernel', 'continuum-live', 'continuum-hands', 'continuum-memory'];

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
    let section = ''; let name = null; const deps = new Set();
    for (const line of strip(read(`${dir}/Cargo.toml`)).split('\n')) {
      const header = line.match(/^\s*\[+([^\]]+)\]+/);
      if (header) { section = header[1].trim(); continue; }
      if (section === 'package' && !name) name = line.match(/^\s*name\s*=\s*"([^"]+)"/)?.[1] ?? null;
      const dependency = /(^|\.)(dev-|build-)?dependencies(\.|$)/.test(section);
      for (const [, dep] of dependency ? line.matchAll(/\bpath\s*=\s*"([^"]+)"/g) : []) {
        deps.add(path.posix.normalize(path.posix.join(dir, dep)));
      }
    }
    if (!name) throw new Error(`Cannot read the package name of ${dir}`);
    return { name, dir, deps: [...deps] };
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
  return { crates: TESTED_CRATES.filter(name => names.has(name)), cli: paths.some(cliPath) };
}

// Only explicitly covered non-Rust surfaces may skip Rust compilation.
// Dependency, build, workflow, generated and unknown changes run the full gates.
// `evidence.workspace` (readWorkspace) narrows Rust to the affected crates on pull requests;
// without it, and on pushes to canary/main (the post-merge net), every crate runs.
function selectChecks(files, evidence = {}) {
  const full = { rust: true, installer: true, publication: true, crates: [...TESTED_CRATES], cli: true };
  if (!Array.isArray(files) || !files.length) return full;
  if (files.some(f => !f || typeof f.filename !== 'string' || !f.filename
      || (f.previous_filename !== undefined && (typeof f.previous_filename !== 'string' || !f.previous_filename)))) return full;
  const paths = files.flatMap(f => [f.filename, f.previous_filename].filter(Boolean));
  const windows = p => /^(install\.ps1|tools\/scripts\/(lib\/(windows-(service|prepared|engine-receipt|elevation|media-reconciliation)|install-common|win-modules|win-engine)\.ps1|tests\/windows-(service|process|airc-download|xz)\.test\.ps1|sync-windows-bootstrap\.ps1|register-core-service\.ps1|run-service-hidden\.ps1))$/.test(p);
  const rustPaths = paths.filter(p => !docs(p) && !windows(p));
  const rust = rustPaths.length > 0;
  const scope = !rust ? { crates: [], cli: false }
    : selectCrates(rustPaths, evidence.eventName === 'push' ? null : evidence.workspace);
  return { rust, installer: rust || paths.some(windows), publication: selectPublication(paths, evidence), ...scope };
}
module.exports = { selectChecks, readWorkspace, TESTED_CRATES };
