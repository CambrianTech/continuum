const { test } = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { selectChecks, readWorkspace, readPolicy, deriveCrates, TESTED_CRATES, TEST_FEATURES, LIB_TESTS } = require('../ci-change-scope.cjs');
const ALL = { crates: TESTED_CRATES, cli: true };
const NONE = { crates: [], cli: false };
const select = (...paths) => {
  const { rust, installer, publication } = selectChecks(paths.map(filename => ({ filename })));
  return { rust, installer, publication };
};

// The tested crates that do not build on the kernel: the floor (base, host, git) and pure leaves
// with no workspace dependency at all (continuum-service). A kernel or lifecycle change cannot
// reach them. Kept explicit, so a crate moving in the graph fails here instead of silently
// changing what CI runs.
// Tested crates a kernel change does not reach: the floor (base, host, git, service) and the leaf
// codec continuum-context, which depends on nothing in the workspace.
const BELOW_KERNEL = ['continuum-base', 'continuum-host', 'continuum-git', 'continuum-service', 'continuum-context', 'continuum-engine-protocol'];

test('installer repairs run installer checks without unrelated Rust builds', () => {
  for (const file of ['install.ps1', 'tools/scripts/lib/install-common.ps1', 'tools/scripts/lib/win-modules.ps1', 'tools/scripts/lib/win-engine.ps1', 'tools/scripts/lib/windows-engine-receipt.ps1', 'tools/scripts/lib/windows-service.ps1', 'tools/scripts/lib/windows-media-reconciliation.ps1', 'tools/scripts/lib/windows-elevation.ps1', 'tools/scripts/tests/windows-process.test.ps1', 'tools/scripts/tests/windows-airc-download.test.ps1', 'tools/scripts/tests/windows-xz.test.ps1', 'tools/scripts/sync-windows-bootstrap.ps1'])
    assert.deepEqual(select(file), { rust: false, installer: true, publication: true });
});
test('unknown, Rust, dependency, workflow and genome inputs retain full gates', () => {
  for (const file of ['core/continuum-core/src/lib.rs', 'Cargo.lock', 'Cargo.toml', '.github/workflows/continuum-rust-tests.yml', 'docs/genome/recipe.md', 'new-installer.ps1', 'tools/scripts/ci-change-scope.cjs'])
    assert.deepEqual(select(file), { rust: true, installer: true, publication: true });
});
test('rename sources cannot hide a Rust deletion in docs', () => {
  assert.deepEqual(selectChecks([{ filename: 'docs/old.md', previous_filename: 'core/old.rs' }]), { rust: true, installer: true, publication: true, ...ALL, bins: {}, examples: {} });
});
test('missing or malformed change evidence runs full gates', () => {
  for (const files of [null, undefined, [], [{}]]) assert.deepEqual(selectChecks(files), { rust: true, installer: true, publication: true, ...ALL, bins: {}, examples: {} });
});
test('mixed changes retain all affected checks', () => {
  assert.deepEqual(select('docs/setup.md', 'install.ps1'), { rust: false, installer: true, publication: true });
  assert.deepEqual(select('install.ps1', 'core/lib.rs'), { rust: true, installer: true, publication: true });
  assert.deepEqual(select('docs/setup.md'), { rust: false, installer: false, publication: true });
});

const publisher = '.github/workflows/core-binaries.yml';
const gate = "${{ !cancelled() && needs.publication-scope.outputs.publication != 'false' }}";
const workflow = () => ({ name: 'core-binaries', on: { push: { branches: ['canary'] } },
  permissions: { contents: 'write' }, concurrency: { 'cancel-in-progress': false },
  jobs: { build: { 'runs-on': 'ubuntu-latest', steps: [{ run: 'cargo build' }] },
    'publish-check': { needs: 'build', steps: [{ run: 'publish' }] } } });
const evidence = (before = workflow(), after = workflow()) => ({ eventName: 'pull_request', complete: true,
  publicationBefore: before, publicationAfter: after });
const publication = (paths, proof) => selectChecks(paths.map(filename => ({ filename })), proof).publication;

test('scheduler-only changes skip artifacts but manual and incomplete evidence build', () => {
  assert.equal(publication(['.github/workflows/core-binaries-hourly.yml'], evidence()), false);
  assert.equal(publication(['tools/scripts/core-binaries-hourly.mjs'], evidence()), false);
  for (const proof of [{}, { eventName: 'workflow_dispatch', complete: true }, { eventName: 'pull_request', complete: false }]) {
    assert.equal(publication([publisher], proof), true);
  }
});

test('trigger-only changes and exact selector wiring skip artifact compilation', () => {
  const before = workflow(); const after = workflow();
  after.on = { workflow_dispatch: null };
  after.jobs['publication-scope'] = { steps: [{ run: 'node --test selector' }] };
  after.jobs.build.needs = 'publication-scope'; after.jobs.build.if = gate;
  assert.equal(publication([publisher, 'docs/activation.md', 'core/continuum-cli-lifecycle/src/prebuilt_artifact/tests.rs'], evidence(before, after)), false);
});

test('protected publication configuration, packages and unknown paths require artifacts', () => {
  const changes = [w => w.jobs.build.steps.push({ run: 'changed compile' }),
    w => w.jobs.build['runs-on'] = 'windows-latest', w => w.env = { FEATURES: 'new' },
    w => w.permissions.contents = 'read', w => w.concurrency['cancel-in-progress'] = true,
    w => w.jobs['publish-check'].steps[0].run = 'different publish',
    w => w.jobs.build.if = 'false', w => w.jobs.build.needs = 'other'];
  for (const change of changes) { const after = workflow(); change(after);
    assert.equal(publication([publisher], evidence(workflow(), after)), true); }
  for (const path of ['tools/scripts/lib/core-features.sh', 'tools/scripts/shared/cargo-features.sh',
    'tools/scripts/tests/publish-core-package.test.sh', 'core/src/lib.rs', 'Cargo.lock', 'unknown']) {
    assert.equal(publication([publisher, path], evidence()), true);
  }
  assert.equal(selectChecks([{ filename: 'docs/moved.md', previous_filename: 'core/lib.rs' }], evidence()).publication, true);
});

test('missing or malformed publication source cannot prove a skip', () => {
  for (const invalid of [undefined, null, [], {}, { jobs: {} }]) {
    assert.equal(publication([publisher], { ...evidence(), publicationAfter: invalid }), true);
  }
  assert.equal(selectChecks([{ filename: publisher, previous_filename: 12 }], evidence()).publication, true);
});

// The crate scope reads the real workspace, so a new member or dependency edge is covered
// the moment it lands in a Cargo.toml: nothing here lists who depends on whom.
const repo = path.resolve(__dirname, '../../..');
const workspace = readWorkspace(repo);
const scope = (paths, eventName = 'pull_request') => {
  const { crates, cli } = selectChecks(paths.map(filename => ({ filename })), { eventName, workspace });
  return { crates, cli };
};
const crates = (...names) => ({ crates: TESTED_CRATES.filter(n => names.includes(n)), cli: false });

test('every tested crate is a workspace member, and the graph names the cut crates', () => {
  const names = workspace.crates.map(c => c.name);
  for (const name of TESTED_CRATES) assert.ok(names.includes(name), name);
  const mind = workspace.crates.find(c => c.name === 'continuum-mind');
  for (const dep of ['core/continuum-academy', 'core/continuum-hands', 'core/continuum-live', 'core/continuum-kernel', 'core/continuum-perception', 'core/continuum-genome']) assert.ok(mind.deps.includes(dep), dep);
});

test('a crate-local change runs that crate and every crate that depends on it', () => {
  assert.deepEqual(scope(['core/continuum-mind/src/persona/unified.rs']), crates('continuum-mind', 'continuum-dreaming', 'continuum-grid', 'continuum-serving', 'continuum-core'));
  assert.deepEqual(scope(['core/continuum-core/src/modules/chat.rs']), crates('continuum-core'));
  assert.deepEqual(scope(['core/continuum-hands/src/lib.rs']), crates('continuum-hands', 'continuum-cognition', 'continuum-academy', 'continuum-mind', 'continuum-dreaming', 'continuum-grid', 'continuum-serving', 'continuum-core'));
  assert.deepEqual(scope(['core/continuum-live/src/lib.rs']), crates('continuum-live', 'continuum-perception', 'continuum-cognition', 'continuum-academy', 'continuum-mind', 'continuum-dreaming', 'continuum-grid', 'continuum-serving', 'continuum-core'));
  assert.deepEqual(scope(['core/continuum-perception/src/lib.rs']), crates('continuum-perception', 'continuum-cognition', 'continuum-academy', 'continuum-mind', 'continuum-dreaming', 'continuum-grid', 'continuum-serving', 'continuum-core'));
  assert.deepEqual(scope(['core/continuum-genome/src/lib.rs']), crates('continuum-genome', 'continuum-store', 'continuum-cognition', 'continuum-academy', 'continuum-mind', 'continuum-dreaming', 'continuum-grid', 'continuum-serving', 'continuum-core'));
  assert.deepEqual(scope(['core/continuum-memory/src/lib.rs']), crates('continuum-memory', 'continuum-embedding', 'continuum-store', 'continuum-cognition', 'continuum-academy', 'continuum-mind', 'continuum-dreaming', 'continuum-grid', 'continuum-serving', 'continuum-core'));
  assert.deepEqual(scope(['core/continuum-experience/src/lib.rs']), crates('continuum-experience', 'continuum-rooms', 'continuum-vitals', 'continuum-capture', 'continuum-cognition', 'continuum-academy', 'continuum-mind', 'continuum-dreaming', 'continuum-grid', 'continuum-serving', 'continuum-core'));
  assert.deepEqual(scope(['core/continuum-cognition/src/lib.rs']), crates('continuum-cognition', 'continuum-academy', 'continuum-mind', 'continuum-dreaming', 'continuum-grid', 'continuum-serving', 'continuum-core'));
  const belowMind = crates('continuum-engine', 'continuum-vitals', 'continuum-prompt', 'continuum-engram', 'continuum-capture', 'continuum-embedding', 'continuum-store', 'continuum-models', 'continuum-hands', 'continuum-live', 'continuum-perception', 'continuum-genome', 'continuum-memory', 'continuum-cognition', 'continuum-academy', 'continuum-sentinel', 'continuum-mind', 'continuum-dreaming', 'continuum-grid', 'continuum-serving', 'continuum-core');
  assert.deepEqual(scope(['core/continuum-engine/src/lib.rs']), belowMind);
  // The floor crates (base, host, git) sit below the kernel: a kernel change runs everything above them.
  const aboveFloor = TESTED_CRATES.filter(n => !BELOW_KERNEL.includes(n));
  assert.deepEqual(scope(['core/continuum-kernel/src/lib.rs']), { crates: aboveFloor, cli: false });
  // A floor change runs the floor crate and every crate built on it.
  assert.deepEqual(scope(['core/continuum-host/src/durable/mod.rs']), { crates: TESTED_CRATES.filter(n => !['continuum-base', 'continuum-service', 'continuum-context', 'continuum-engine-protocol'].includes(n)), cli: true });
  // continuum-git is used by hands (a citizen's git) and core (the deploy tree): both run, and every crate above hands.
  assert.deepEqual(scope(['core/continuum-git/src/index_lock.rs']), crates('continuum-git', 'continuum-hands', 'continuum-cognition', 'continuum-academy', 'continuum-mind', 'continuum-dreaming', 'continuum-grid', 'continuum-serving', 'continuum-core'));
  // Untested crates reach the tested ones through the graph: llama -> engine -> ...
  assert.deepEqual(scope(['core/llama/src/lib.rs']), belowMind);
  assert.deepEqual(scope(['core/continuum-mind/src/a.rs', 'core/continuum-live/src/b.rs']), crates('continuum-live', 'continuum-perception', 'continuum-cognition', 'continuum-academy', 'continuum-mind', 'continuum-dreaming', 'continuum-grid', 'continuum-serving', 'continuum-core'));
  // A crate's own manifest is that crate's change; a rename counts both ends.
  assert.deepEqual(scope(['core/continuum-hands/Cargo.toml']), crates('continuum-hands', 'continuum-cognition', 'continuum-academy', 'continuum-mind', 'continuum-dreaming', 'continuum-grid', 'continuum-serving', 'continuum-core'));
  const { crates: moved } = selectChecks([{ filename: 'core/continuum-mind/src/x.rs', previous_filename: 'core/continuum-engine/src/x.rs' }], { eventName: 'pull_request', workspace });
  assert.ok(moved.includes('continuum-engine'));
});

test('a crate no tested crate depends on runs no crate job', () => {
  for (const file of ['core/jtag-mcp/src/main.rs', 'apps/cli/src/main.rs', 'client/continuum-client-ffi/src/lib.rs']) assert.deepEqual(scope([file]), NONE);
});

test('workspace-wide, build, vendor and unknown inputs select every crate', () => {
  for (const file of ['Cargo.lock', 'Cargo.toml', 'rust-toolchain.toml', 'core/.cargo/config.toml', '.cargo/config.toml',
    'core/continuum-core/build.rs', 'core/llama/build.rs', 'core/vendor/llama.cpp', 'core/vendor/llama.cpp/ggml/src/ggml.c',
    '.github/workflows/continuum-rust-tests.yml', 'tools/scripts/ci-change-scope.cjs', 'protocol/typescript/x.ts',
    'docs/genome/coder-eval.jsonl', 'core/README-not-md.txt', 'unknown'])
    assert.deepEqual(scope([file]), ALL, file);
  assert.deepEqual(scope(['core/continuum-mind/src/a.rs', 'Cargo.lock']), ALL);
});

test('docs, markdown and installer-only changes run no crate', () => {
  for (const file of ['docs/setup.md', 'core/continuum-mind/README.md', 'install.ps1']) assert.deepEqual(scope([file]), NONE, file);
  assert.deepEqual(scope(['docs/setup.md', 'core/continuum-mind/src/a.rs']), crates('continuum-mind', 'continuum-dreaming', 'continuum-grid', 'continuum-serving', 'continuum-core'));
});

test('CLI handoff regressions run for the CLI and lifecycle paths, or with everything', () => {
  assert.deepEqual(scope(['core/continuum-core/src/bin/continuum/reboot.rs']), { crates: ['continuum-core'], cli: true });
  assert.deepEqual(scope(['core/continuum-core/src/bin/continuum.rs']), { crates: ['continuum-core'], cli: true });
  assert.deepEqual(scope(['core/continuum-cli-lifecycle/src/lib.rs']), { crates: TESTED_CRATES.filter(n => !BELOW_KERNEL.includes(n)), cli: true });
  assert.equal(scope(['core/continuum-core/src/bin/continuum_other.rs']).cli, false);
  assert.equal(scope(['core/continuum-mind/src/a.rs']).cli, false);
});

test('a PR that touches an example builds it, and runs the tests of one declaring them', () => {
  // what this catches: a PR breaking examples/replay reaching canary uncompiled, because every
  // crate job builds only the lib (#5511, Cormac on #5611: E0425 in examples/replay/sediment.rs).
  const examples = (paths, eventName = 'pull_request') =>
    selectChecks(paths.map(filename => ({ filename })), { eventName, workspace }).examples;
  const context = workspace.crates.find(c => c.name === 'continuum-context');
  assert.ok(context.examples.some(e => e.name === 'replay' && e.test), 'replay declares test = true');
  // A module file under examples/<name>/ names the example; the implicit summarize is built, not tested.
  assert.deepEqual(examples(['core/continuum-context/examples/replay/sediment.rs', 'core/continuum-context/examples/summarize.rs']),
    { 'continuum-context': { all: ['replay', 'summarize'], tested: ['replay'] } });
  // An example whose required features the crate's test build does not enable is not selected.
  const core = workspace.crates.find(c => c.name === 'continuum-core');
  assert.ok(core.examples.some(e => e.name === 'avatar_breathe' && e.requiredFeatures.includes('avatar-3d')));
  assert.deepEqual(examples(['core/continuum-core/examples/avatar_breathe.rs'])['continuum-core'],
    TEST_FEATURES['continuum-core'].includes('avatar-3d') ? { all: ['avatar_breathe'], tested: [] } : undefined);
  // A lib change, docs and pushes build no example.
  assert.deepEqual(examples(['core/continuum-context/src/source.rs']), {});
  assert.deepEqual(examples(['docs/a.md']), {});
  assert.deepEqual(examples(['core/continuum-context/examples/replay.rs'], 'push'), {});
});

test('a PR that touches a declared binary\'s own source compiles that binary', () => {
  // what this catches: a PR editing core/continuum-core/src/main.rs reaching canary with no
  // compile of continuum-core-server, because every crate job builds only the lib (card 67bf5d81).
  const bins = (paths, eventName = 'pull_request') =>
    selectChecks(paths.map(filename => ({ filename })), { eventName, workspace }).bins;
  const core = workspace.crates.find(c => c.name === 'continuum-core');
  assert.ok(core.bins.some(b => b.name === 'continuum-core-server' && b.path === 'src/main.rs'));
  assert.deepEqual(bins(['core/continuum-core/src/main.rs']), { 'continuum-core': ['continuum-core-server'] });
  assert.deepEqual(bins(['core/continuum-core/src/bin/forge_custodian.rs', 'core/continuum-core/src/main.rs']),
    { 'continuum-core': ['continuum-core-server', 'forge-custodian'] });
  // The manifest declares the bins: changing it checks every declared one but the CLI.
  assert.deepEqual(bins(['core/continuum-core/Cargo.toml'])['continuum-core'],
    core.bins.map(b => b.name).filter(name => name !== 'continuum'));
  // A lib change, the CLI (compiled by the handoff job), docs, and pushes compile no extra binary.
  assert.deepEqual(bins(['core/continuum-core/src/modules/chat.rs']), {});
  assert.deepEqual(bins(['core/continuum-core/src/bin/continuum/reboot.rs']), {});
  assert.deepEqual(bins(['docs/a.md']), {});
  assert.deepEqual(bins(['core/continuum-core/src/main.rs'], 'push'), {});
});

test('pushes to canary/main and a missing workspace run every crate', () => {
  assert.deepEqual(scope(['core/continuum-mind/src/a.rs'], 'push'), ALL);
  const { crates: c, cli } = selectChecks([{ filename: 'core/continuum-mind/src/a.rs' }], { eventName: 'pull_request' });
  assert.deepEqual({ crates: c, cli }, ALL);
  assert.deepEqual(scope(['docs/a.md'], 'push'), NONE);
});

test('a malformed workspace cannot be read as a narrower scope', () => {
  const fs = require('node:fs'); const os = require('node:os');
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ws-'));
  fs.writeFileSync(path.join(root, 'Cargo.toml'), '[workspace]\nmembers = ["a"]\n');
  fs.mkdirSync(path.join(root, 'a'));
  fs.writeFileSync(path.join(root, 'a/Cargo.toml'), '[dependencies]\nx = 1\n');
  assert.throws(() => readWorkspace(root), /package name/);
  fs.writeFileSync(path.join(root, 'Cargo.toml'), '[workspace]\n');
  assert.throws(() => readWorkspace(root), /members/);
  fs.rmSync(root, { recursive: true });
});

test('every tested crate names the features its tests build with', () => {
  // what this catches: a crate added to TESTED_CRATES whose job and cache warm-up would build
  // it without its fixtures, or two different feature sets for the two.
  assert.deepEqual(Object.keys(TEST_FEATURES), TESTED_CRATES);
  for (const crate of TESTED_CRATES) assert.equal(typeof TEST_FEATURES[crate], 'string', crate);
});

test('a crate dropped into the workspace is tested, featured and counted with no list edited', () => {
  // what this catches: the hand-kept crate lists (review missed an entry twice on 2026-10-26,
  // #5456 and #5465): a new member must reach the matrix, its features, the generic lib-test
  // step and the warnings ratchet from `members` alone.
  const fs = require('node:fs'); const os = require('node:os');
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ws-'));
  const crate = (dir, name, extra = '') => {
    fs.mkdirSync(path.join(root, dir), { recursive: true });
    fs.writeFileSync(path.join(root, dir, 'Cargo.toml'), `[package]\nname = "${name}"\n${extra}`);
  };
  fs.writeFileSync(path.join(root, 'Cargo.toml'), `[workspace]
members = ["core/continuum-core", "core/continuum-brand-new", "core/continuum-plain", "core/vendored-thing", "apps/cli"]
[workspace.metadata.ci]
by_nature = ["vendored-thing"]
pending = []
own_lib_test_step = ["continuum-core"]
longest_first = ["continuum-core"]
split_prefixed = []
unaudited = []
warnings_uncounted = ["continuum-plain"]
`);
  crate('core/continuum-core', 'continuum-core');
  crate('core/continuum-brand-new', 'continuum-brand-new', '[features]\ntest-fixtures = []\n');
  crate('core/continuum-plain', 'continuum-plain');
  crate('core/vendored-thing', 'vendored-thing');
  crate('apps/cli', 'cli');
  const got = deriveCrates(readWorkspace(root), readPolicy(root));
  assert.deepEqual(got.tested, ['continuum-core', 'continuum-brand-new', 'continuum-plain']);
  assert.equal(got.features['continuum-brand-new'], 'test-fixtures');
  assert.equal(got.features['continuum-plain'], '');
  assert.deepEqual(got.libTests, { 'continuum-core': false, 'continuum-brand-new': true, 'continuum-plain': true });
  assert.deepEqual(got.warned, ['continuum-core', 'continuum_core', 'continuum-brand-new', 'continuum_brand_new']);
  fs.writeFileSync(path.join(root, 'Cargo.toml'), '[workspace]\nmembers = []\n');
  assert.throws(() => readPolicy(root), /by_nature/);
  fs.rmSync(root, { recursive: true });
});

test('the real workspace derives a lib-test flag for every tested crate', () => {
  assert.deepEqual(Object.keys(LIB_TESTS), TESTED_CRATES);
  for (const own of readPolicy(repo).own_lib_test_step) assert.equal(LIB_TESTS[own], false, own);
});


test('client transport changes execute client lib tests through the CLI job', () => {
  assert.equal(scope(['client/continuum-client/src/airc_ipc.rs']).cli, true);
  assert.equal(scope(['client/continuum-client/Cargo.toml']).cli, true);
});


test('CRLF workspace comments do not become phantom crate members', () => {
  const fs = require('node:fs'); const os = require('node:os');
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ws-crlf-'));
  fs.writeFileSync(path.join(root, 'Cargo.toml'), '[workspace]\r\nmembers = [\r\n # "core/removed"\r\n]\r\n');
  assert.deepEqual(readWorkspace(root).crates, []);
  fs.rmSync(root, { recursive: true });
});
