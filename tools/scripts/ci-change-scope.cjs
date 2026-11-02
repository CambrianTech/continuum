const fs = require('node:fs');
const path = require('node:path');
const { isDeepStrictEqual } = require('node:util');

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
  const strip = text => text.split(/\r?\n/).map(line => line.replace(/#.*$/, '')).join('\n');
  const members = strip(read('Cargo.toml')).match(/^\s*members\s*=\s*\[([^\]]*)\]/m);
  if (!members) throw new Error('Cannot read workspace members from Cargo.toml');
  const crates = [...members[1].matchAll(/"([^"]+)"/g)].map(([, dir]) => {
    let section = ''; let name = null; const deps = new Set(); const bins = []; const examples = []; const features = [];
    for (const line of strip(read(`${dir}/Cargo.toml`)).split('\n')) {
      const header = line.match(/^\s*\[+([^\]]+)\]+/);
      if (header) {
        section = header[1].trim();
        if (section === 'bin') bins.push({});
        if (section === 'example') examples.push({ requiredFeatures: [], test: false });
        continue;
      }
      if (section === 'package' && !name) name = line.match(/^\s*name\s*=\s*"([^"]+)"/)?.[1] ?? null;
      if (section === 'features') { const f = line.match(/^\s*([A-Za-z0-9_-]+)\s*=/); if (f) features.push(f[1]); }
      const field = section === 'bin' && line.match(/^\s*(name|path)\s*=\s*"([^"]+)"/);
      if (field) bins[bins.length - 1][field[1]] = field[2];
      if (section === 'example') {
        const example = examples[examples.length - 1];
        const named = line.match(/^\s*name\s*=\s*"([^"]+)"/);
        if (named) example.name = named[1];
        const required = line.match(/^\s*required-features\s*=\s*\[([^\]]*)\]/);
        if (required) example.requiredFeatures = [...required[1].matchAll(/"([^"]+)"/g)].map(([, f]) => f);
        if (/^\s*test\s*=\s*true\b/.test(line)) example.test = true;
      }
      const dependency = /(^|\.)(dev-|build-)?dependencies(\.|$)/.test(section);
      for (const [, dep] of dependency ? line.matchAll(/\bpath\s*=\s*"([^"]+)"/g) : []) {
        deps.add(path.posix.normalize(path.posix.join(dir, dep)));
      }
    }
    if (!name) throw new Error(`Cannot read the package name of ${dir}`);
    return { name, dir, deps: [...deps], features, bins: bins.filter(b => b.name && b.path), examples: examples.filter(e => e.name) };
  });
  return { crates };
}

// The crate policy: the exceptions named under [workspace.metadata.ci] in the root Cargo.toml,
// the one place crates are named (see the comments there). Arrays of quoted names, by key.
function readPolicy(root = '.') {
  let section = ''; let body = '';
  for (const line of fs.readFileSync(path.join(root, 'Cargo.toml'), 'utf8').split('\n')) {
    const header = line.match(/^\s*\[+([^\]]+)\]+/);
    if (header) section = header[1].trim();
    else if (section === 'workspace.metadata.ci') body += `${line.replace(/#.*$/, '')}\n`;
  }
  const policy = Object.fromEntries([...body.matchAll(/(\w+)\s*=\s*\[([^\]]*)\]/g)]
    .map(([, key, list]) => [key, [...list.matchAll(/"([^"]+)"/g)].map(([, name]) => name)]));
  for (const key of ['by_nature', 'pending', 'own_lib_test_step', 'longest_first', 'split_prefixed', 'unaudited', 'warnings_uncounted']) {
    if (!Array.isArray(policy[key])) throw new Error(`Cannot read ${key} from [workspace.metadata.ci] in Cargo.toml`);
  }
  policy.untested = [...policy.by_nature, ...policy.pending];
  return policy;
}

// Core is the one crate that tests the call (livekit-webrtc) and 3D-avatar (avatar-3d) paths a
// node's profile turns on; every other crate builds with its own `test-fixtures` feature, if any.
const FEATURE_OVERRIDES = { 'continuum-core': 'livekit-webrtc,avatar-3d' };

// Everything CI derives from the workspace: no crate is listed here. Every `core/` member is
// tested unless the policy excepts it; the crate job and the cache-warming job both read `features`.
function deriveCrates(workspace, policy) {
  const members = workspace.crates.filter(c => c.dir.startsWith('core/') && !policy.untested.includes(c.name));
  const first = policy.longest_first.filter(name => members.some(c => c.name === name));
  const tested = [...first, ...members.map(c => c.name).filter(name => !first.includes(name))];
  const features = Object.fromEntries(tested.map(name => [name, FEATURE_OVERRIDES[name]
    ?? (members.find(c => c.name === name).features.includes('test-fixtures') ? 'test-fixtures' : '')]));
  const libTests = Object.fromEntries(tested.map(name => [name, !policy.own_lib_test_step.includes(name)]));
  // What the warnings ratchet counts: both spellings, since rustc reports the underscore form.
  const warned = tested.filter(name => !policy.warnings_uncounted.includes(name));
  // The per-file ratchets audit (by directory) every tested crate but the unaudited: the split
  // crates prefixed, the others cut from core unprefixed.
  const audited = members.filter(c => !policy.unaudited.includes(c.name));
  const split = audited.filter(c => policy.split_prefixed.includes(c.name)).map(c => path.posix.basename(c.dir));
  const modules = audited.filter(c => c.name !== 'continuum-core' && !policy.split_prefixed.includes(c.name)).map(c => path.posix.basename(c.dir));
  return { tested, features, libTests, split, modules, warned: warned.flatMap(name => [name, name.replace(/-/g, '_')]) };
}

const repoRoot = path.resolve(__dirname, '../..');
const derived = deriveCrates(readWorkspace(repoRoot), readPolicy(repoRoot));
const TESTED_CRATES = derived.tested;
// The cargo features each crate's job builds its tests with (see deriveCrates).
const TEST_FEATURES = derived.features;
// Whether the generic `cargo test -p <crate> --lib` step runs it (false: its own steps do).
const LIB_TESTS = derived.libTests;

// Paths whose effect on compilation is not confined to one crate: they select every crate.
const global = p => p === 'Cargo.lock' || p === 'Cargo.toml' || p === 'rust-toolchain.toml'
  || /(^|\/)\.cargo\//.test(p) || /(^|\/)build\.rs$/.test(p) || p.startsWith('core/vendor/');
// The installed-release handoff regressions exercise the CLI binary and its lifecycle crate.
const cliPath = p => p.startsWith('core/continuum-cli-lifecycle/')
  || p.startsWith('client/continuum-client/')
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

// The non-lib targets of tested crates whose own source a change touches. Lib and lib-test builds
// compile neither, so without this a PR editing `src/main.rs` (card 67bf5d81) or an example
// (#5511, #5611) reached canary uncompiled. A change to the lib a target uses is not covered here;
// the hourly core-binaries publication compiles every bin.
//
// `bins`: the declared binaries (`[[bin]]`) whose file is touched, and for `src/bin/<x>.rs` its
// module directory `src/bin/<x>/`; a change to the crate's Cargo.toml, where the declarations live,
// selects all of them. Implicit (undeclared) bins are not read. The CLI binary is left to the
// handoff job, which already compiles it.
//
// `examples`: `examples/<name>.rs` or anything under `examples/<name>/` names the example `<name>`,
// declared or implicit. One whose `required-features` the crate's test build does not enable is
// not selected (cargo refuses to build it). `tested` lists those declaring `test = true`, whose own
// tests run as well.
function selectTargets(paths, workspace, crates) {
  const bins = {}; const examples = {};
  for (const c of workspace?.crates ?? []) {
    if (!crates.includes(c.name)) continue;
    const touchedBins = (c.bins ?? []).filter(b => {
      const file = path.posix.join(c.dir, b.path);
      const moduleDir = file.endsWith('.rs') && path.posix.basename(path.posix.dirname(file)) === 'bin' ? `${file.slice(0, -3)}/` : null;
      return paths.some(p => p === file || p === `${c.dir}/Cargo.toml` || (moduleDir && p.startsWith(moduleDir)));
    }).map(b => b.name).filter(name => !(c.name === 'continuum-core' && name === 'continuum'));
    if (touchedBins.length) bins[c.name] = touchedBins;
    const enabled = (TEST_FEATURES[c.name] ?? '').split(/[\s,]+/).filter(Boolean);
    const names = new Set(paths.filter(p => p.startsWith(`${c.dir}/examples/`))
      .map(p => p.slice(`${c.dir}/examples/`.length).split('/')[0].replace(/\.rs$/, '')));
    const declared = name => (c.examples ?? []).find(e => e.name === name);
    const buildable = [...names].sort().filter(name => (declared(name)?.requiredFeatures ?? []).every(f => enabled.includes(f)));
    if (buildable.length) examples[c.name] = { all: buildable, tested: buildable.filter(name => declared(name)?.test) };
  }
  return { bins, examples };
}

// Only explicitly covered non-Rust surfaces may skip Rust compilation.
// Dependency, build, workflow, generated and unknown changes run the full gates.
// `evidence.workspace` (readWorkspace) narrows Rust to the affected crates on pull requests;
// without it, and on pushes to canary/main (the post-merge net), every crate runs.
function selectChecks(files, evidence = {}) {
  const full = { rust: true, installer: true, publication: true, crates: [...TESTED_CRATES], cli: true, bins: {}, examples: {} };
  if (!Array.isArray(files) || !files.length) return full;
  if (files.some(f => !f || typeof f.filename !== 'string' || !f.filename
      || (f.previous_filename !== undefined && (typeof f.previous_filename !== 'string' || !f.previous_filename)))) return full;
  const paths = files.flatMap(f => [f.filename, f.previous_filename].filter(Boolean));
  const windows = p => /^(install\.ps1|tools\/scripts\/(lib\/(windows-(service|prepared|engine-receipt|elevation|media-reconciliation)|install-common|win-modules|win-engine)\.ps1|tests\/windows-(service|process|airc-download|xz)\.test\.ps1|sync-windows-bootstrap\.ps1|register-core-service\.ps1|run-service-hidden\.ps1))$/.test(p);
  const rustPaths = paths.filter(p => !docs(p) && !windows(p));
  const rust = rustPaths.length > 0;
  const workspace = evidence.eventName === 'push' ? null : evidence.workspace;
  const scope = !rust ? { crates: [], cli: false } : selectCrates(rustPaths, workspace);
  const targets = selectTargets(rustPaths, workspace, scope.crates);
  return { rust, installer: rust || paths.some(windows), publication: selectPublication(paths, evidence), ...scope, ...targets };
}
module.exports = { selectChecks, readWorkspace, readPolicy, deriveCrates, TESTED_CRATES, TEST_FEATURES, LIB_TESTS };

// Shell and Rust readers take the derived sets from here, as JSON: `warning-crates` (the warnings
// ratchet), `crate-sets` (source-hygiene's guard, which checks its own Rust derivation against it).
if (require.main === module && process.argv[2] === 'warning-crates') console.log(JSON.stringify(derived.warned));
if (require.main === module && process.argv[2] === 'crate-sets') {
  console.log(JSON.stringify({ tested: derived.tested, warned: derived.warned, split: derived.split, modules: derived.modules }));
}
