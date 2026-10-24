const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { SETTLED, cacheCommit, parseNameStatus, plan, settle, testExecutables } = require('../ci-cargo-freshness.cjs');

const PREFIX = 'continuum-rust-target-v3-Linux-abc-';
const A = 'a'.repeat(40);

test('only a key of ours ending in a full commit names the cache commit', () => {
  assert.equal(cacheCommit(PREFIX + A, PREFIX), A);
  for (const key of [undefined, '', PREFIX, PREFIX + 'abc', PREFIX + A.toUpperCase(), 'other-' + A, PREFIX + A + 'x'])
    assert.equal(cacheCommit(key, PREFIX), null, String(key));
  assert.equal(cacheCommit(PREFIX + A, ''), null);
});

test('name-status records pair up, and a torn record is refused', () => {
  assert.deepEqual(parseNameStatus('M\0a.rs\0D\0b c.rs\0'), [{ status: 'M', path: 'a.rs' }, { status: 'D', path: 'b c.rs' }]);
  assert.throws(() => parseNameStatus('M\0a.rs\0D\0'));
});

test('every kind of difference reads as changed', () => {
  const kinds = { 'core/a/src/x.rs': 'file', 'core/vendor/lib': 'dir', 'core/a/src/gone.rs': 'missing',
    'core/a/src': 'dir', 'core/a': 'dir', core: 'dir', 'core/a/data/t.json': 'file', 'core/a/README.md': 'file',
    'core/old/x.rs': 'missing', 'core/old': 'missing' };
  const work = plan(['core/a/src/x.rs', 'core/vendor/lib', 'core/a/src/gone.rs', 'core/a/data/t.json', 'core/a/README.md', 'core/old/x.rs']
    .map(p => ({ status: 'M', path: p })), { kind: p => kinds[p] ?? 'missing', crateDirs: ['core/a', 'core/ab'] });
  assert.deepEqual(work.files.sort(), ['core/a/README.md', 'core/a/data/t.json', 'core/a/src/x.rs']);
  assert.deepEqual(work.trees, ['core/vendor/lib']);
  // A deletion marks every existing ancestor directory, skipping ones deleted with it.
  assert.deepEqual(work.dirs.sort(), ['core', 'core/a', 'core/a/src']);
  // A non-Rust, non-markdown input recompiles its crate; Rust and markdown inputs are tracked by dep-info.
  assert.deepEqual(work.crates, ['core/a']);
});

test('compiler-artifact test executables are the only ones pruned', () => {
  const lines = [
    { reason: 'compiler-artifact', profile: { test: true }, executable: '/t/deps/core-1' },
    { reason: 'compiler-artifact', profile: { test: false }, executable: '/t/deps/bin-2' },
    { reason: 'compiler-artifact', profile: { test: true }, executable: null },
    { reason: 'build-script-executed', out_dir: '/t/build/x' },
  ].map(m => JSON.stringify(m)).join('\n');
  assert.deepEqual(testExecutables(`   Compiling x\n${lines}\nnot json {\n`), ['/t/deps/core-1']);
});

// A real repository: the cache was built at commit C; HEAD changes a lower crate, deletes a
// module, edits a crate's data file. Each must read newer than any cached output; everything
// else must read older.
test('settle against a real cache commit: changes read new, the rest settles', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'freshness-'));
  const cargoHome = fs.mkdtempSync(path.join(os.tmpdir(), 'cargo-home-'));
  const sh = (...args) => execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();
  const write = (p, text) => { fs.mkdirSync(path.dirname(path.join(root, p)), { recursive: true }); fs.writeFileSync(path.join(root, p), text); };
  try {
    sh('init', '-q'); sh('config', 'user.email', 't@t'); sh('config', 'user.name', 't');
    write('core/low/src/lib.rs', 'pub fn f() {}\n'); write('core/low/src/old.rs', '\n'); write('core/high/src/lib.rs', 'use low::f;\n');
    write('core/high/data/t.json', '{}\n'); write('docs/a.md', 'a\n');
    sh('add', '-A'); sh('commit', '-qm', 'C'); const C = sh('rev-parse', 'HEAD');
    write('core/low/src/lib.rs', 'pub fn f() -> u8 { 1 }\n'); fs.unlinkSync(path.join(root, 'core/low/src/old.rs'));
    write('core/high/data/t.json', '{"a":1}\n');
    sh('add', '-A'); sh('commit', '-qm', 'HEAD');
    fs.mkdirSync(path.join(cargoHome, 'registry/src/index/ring-0.17'), { recursive: true });
    fs.writeFileSync(path.join(cargoHome, 'registry/src/index/ring-0.17/build.rs'), '');
    const before = Date.now() - 1000;
    const mtime = p => fs.statSync(path.join(root, p)).mtimeMs;
    const result = settle({ root, prefix: PREFIX, matchedKey: PREFIX + C, cargoHome, crateDirs: ['core/low', 'core/high'], log: () => {} });
    assert.equal(result.settled, true);
    for (const p of ['core/low/src/lib.rs', 'core/low/src', 'core/high/data/t.json', 'core/high/src/lib.rs'])
      assert.ok(mtime(p) >= before, `${p} must read as changed`);
    for (const p of ['docs/a.md', 'core/high/src', 'core/low/Cargo.toml'].filter(p => fs.existsSync(path.join(root, p))))
      assert.equal(mtime(p), SETTLED.getTime(), `${p} must settle`);
    assert.equal(fs.statSync(path.join(cargoHome, 'registry/src/index/ring-0.17/build.rs')).mtimeMs, SETTLED.getTime());
    // An unknown cache commit settles nothing in the workspace: everything rebuilds.
    write('core/high/src/lib.rs', 'use low::f; // edit\n');
    const cold = settle({ root, prefix: PREFIX, matchedKey: PREFIX + 'f'.repeat(40), cargoHome, crateDirs: [], log: () => {} });
    assert.equal(cold.settled, false);
    assert.ok(mtime('core/high/src/lib.rs') >= before);
    assert.equal(settle({ root, prefix: PREFIX, matchedKey: '', cargoHome, crateDirs: [], log: () => {} }).settled, false);
  } finally {
    fs.rmSync(root, { recursive: true, force: true }); fs.rmSync(cargoHome, { recursive: true, force: true });
  }
});
