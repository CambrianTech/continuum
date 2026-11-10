import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { resolveBuildKey } from '../publication-build-key.mjs';

const execute = promisify(execFile);
test('real git history resolves docs tips to immutable build inputs', async () => {
  const cwd = await mkdtemp(join(tmpdir(), 'continuum-publication-key-'));
  const git = async (...args) => (await execute('git', args, { cwd })).stdout.trim();
  try {
    await git('init');
    await git('config', 'user.name', 'Publication test');
    await git('config', 'user.email', 'publication@example.invalid');
    const commit = async (path, body) => {
      await writeFile(join(cwd, path), body);
      await git('add', path);
      await git('-c', 'commit.gpgsign=false', 'commit', '-m', path);
      return git('rev-parse', 'HEAD');
    };
    await mkdir(join(cwd, 'core'));
    const first = await commit('core/input.rs', 'first');
    const docs = await commit('README.md', 'docs only');
    assert.equal(await resolveBuildKey(docs, cwd), first);
    const second = await commit('Cargo.toml', 'new build');
    assert.equal(await resolveBuildKey(second, cwd), second);
    assert.equal(await resolveBuildKey(docs, cwd), first, 'a later tip cannot move a pinned key');
    await assert.rejects(resolveBuildKey('canary', cwd), /full SHA/);
    await assert.rejects(resolveBuildKey('f'.repeat(40), cwd));
  } finally {
    await rm(cwd, { recursive: true, force: true });
  }
});
