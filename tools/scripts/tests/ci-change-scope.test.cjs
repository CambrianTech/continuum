const { test } = require('node:test');
const assert = require('node:assert/strict');
const { selectChecks } = require('../ci-change-scope.cjs');
const select = (...paths) => selectChecks(paths.map(filename => ({ filename })));
test('installer repairs run installer checks without unrelated Rust builds', () => {
  for (const file of ['install.ps1', 'tools/scripts/lib/install-common.ps1', 'tools/scripts/lib/win-modules.ps1', 'tools/scripts/lib/windows-engine-receipt.ps1', 'tools/scripts/lib/windows-service.ps1'])
    assert.deepEqual(select(file), { rust: false, installer: true });
});
test('unknown, Rust, dependency, workflow and genome inputs retain full gates', () => {
  for (const file of ['core/continuum-core/src/lib.rs', 'Cargo.lock', 'Cargo.toml', '.github/workflows/continuum-rust-tests.yml', 'docs/genome/recipe.md', 'new-installer.ps1', 'tools/scripts/ci-change-scope.cjs'])
    assert.deepEqual(select(file), { rust: true, installer: true });
});
test('rename sources cannot hide a Rust deletion in docs', () => {
  assert.deepEqual(selectChecks([{ filename: 'docs/old.md', previous_filename: 'core/old.rs' }]), { rust: true, installer: true });
});
test('missing or malformed change evidence runs full gates', () => {
  for (const files of [null, undefined, [], [{}]]) assert.deepEqual(selectChecks(files), { rust: true, installer: true });
});
test('mixed changes retain all affected checks', () => {
  assert.deepEqual(select('docs/setup.md', 'install.ps1'), { rust: false, installer: true });
  assert.deepEqual(select('install.ps1', 'core/lib.rs'), { rust: true, installer: true });
  assert.deepEqual(select('docs/setup.md'), { rust: false, installer: false });
});
