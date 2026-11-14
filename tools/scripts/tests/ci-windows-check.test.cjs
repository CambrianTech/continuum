const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const root = path.resolve(__dirname, '../../..');
const workflow = fs.readFileSync(path.join(root, '.github/workflows/continuum-rust-tests.yml'), 'utf8');
const gate = workflow.slice(workflow.indexOf('  cargo-check-windows:'));
const run = gate.split('        run: |')[1].split('\n').filter(line => line.startsWith('          ')).map(line => line.slice(10)).join('\n');
function verdict(overrides) {
  const result = spawnSync(process.env.CI_TEST_BASH || 'bash', ['-e', '-c', run], {
    env: { ...process.env, SELECT_RESULT: 'success', RUST_CHANGED: 'true', CHECK_RESULT: 'success', CORE_RESULT: 'skipped', LIFECYCLE_RESULT: 'skipped', ...overrides }, encoding: 'utf8'
  });
  assert.ifError(result.error);
  if (result.status !== 0 && result.stderr) process.stderr.write(result.stderr);
  return result.status;
}
test('selected Windows compilation cannot pass through skipped, failed, or cancelled jobs', () => {
  // Regression: the required context used to pass when both native jobs skipped.
  assert.equal(verdict({}), 0);
  for (const CHECK_RESULT of ['skipped', 'failure', 'cancelled', '']) assert.notEqual(verdict({ CHECK_RESULT }), 0);
  for (const SELECT_RESULT of ['skipped', 'failure', 'cancelled']) assert.notEqual(verdict({ SELECT_RESULT }), 0);
  for (const RUST_CHANGED of ['', 'unknown']) assert.notEqual(verdict({ RUST_CHANGED }), 0);
});
test('docs may skip compilation but existing custody failures still fail the gate', () => {
  // Regression: adding the fast compile must not hide an existing native-contract failure.
  assert.equal(verdict({ RUST_CHANGED: 'false', CHECK_RESULT: 'skipped' }), 0);
  for (const field of ['CORE_RESULT', 'LIFECYCLE_RESULT']) {
    for (const value of ['failure', 'cancelled']) assert.notEqual(verdict({ [field]: value }), 0);
  }
});
