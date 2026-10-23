const { test } = require('node:test');
const assert = require('node:assert/strict');
const { selectChecks } = require('../ci-change-scope.cjs');
const select = (...paths) => selectChecks(paths.map(filename => ({ filename })));
test('installer repairs run installer checks without unrelated Rust builds', () => {
  for (const file of ['install.ps1', 'tools/scripts/lib/install-common.ps1', 'tools/scripts/lib/win-modules.ps1', 'tools/scripts/lib/win-engine.ps1', 'tools/scripts/lib/windows-engine-receipt.ps1', 'tools/scripts/lib/windows-service.ps1', 'tools/scripts/lib/windows-media-reconciliation.ps1', 'tools/scripts/lib/windows-elevation.ps1', 'tools/scripts/tests/windows-process.test.ps1', 'tools/scripts/tests/windows-airc-download.test.ps1', 'tools/scripts/tests/windows-xz.test.ps1', 'tools/scripts/sync-windows-bootstrap.ps1'])
    assert.deepEqual(select(file), { rust: false, installer: true, publication: true });
});
test('unknown, Rust, dependency, workflow and genome inputs retain full gates', () => {
  for (const file of ['core/continuum-core/src/lib.rs', 'Cargo.lock', 'Cargo.toml', '.github/workflows/continuum-rust-tests.yml', 'docs/genome/recipe.md', 'new-installer.ps1', 'tools/scripts/ci-change-scope.cjs'])
    assert.deepEqual(select(file), { rust: true, installer: true, publication: true });
});
test('rename sources cannot hide a Rust deletion in docs', () => {
  assert.deepEqual(selectChecks([{ filename: 'docs/old.md', previous_filename: 'core/old.rs' }]), { rust: true, installer: true, publication: true });
});
test('missing or malformed change evidence runs full gates', () => {
  for (const files of [null, undefined, [], [{}]]) assert.deepEqual(selectChecks(files), { rust: true, installer: true, publication: true });
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
