import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { platforms, runPublication } from '../core-binaries-hourly.mjs';

const sha = '0123456789abcdef0123456789abcdef01234567';
function source({ complete = false, running = false, badManifest = false, fail = false } = {}) {
  const calls = [];
  const manifests = new Map();
  const assets = platforms.flatMap((platform, index) => {
    manifests.set(index * 3 + 2, { git_sha: badManifest ? 'stale' : sha, platform,
      archive: `continuum-core-${platform}.tar.gz`, sha256: 'a'.repeat(64) });
    return ['.tar.gz', '.tar.gz.sha256', '.json'].map((suffix, offset) => ({
      name: `continuum-core-${platform}${suffix}`, id: index * 3 + offset, size: 10,
    }));
  });
  const api = async (path, options = {}) => {
    calls.push({ path, options });
    if (fail) throw new Error('HTTP 503');
    if (path.endsWith('/commits/canary')) return { sha };
    if (path.includes('/releases/tags/')) return complete ? { assets } : null;
    if (path.includes('/releases/assets/')) return manifests.get(Number(path.split('/').at(-1)));
    if (path.includes('/runs?')) return { workflow_runs: running ? [{ head_sha: sha, event: 'workflow_dispatch', status: 'in_progress' }] : [] };
    if (path.endsWith('/dispatches')) return null;
    throw new Error(`unexpected request ${path}`);
  };
  return { api, calls };
}

test('unpublished canary dispatches explicit canary; dry run never dispatches', async () => {
  for (const dryRun of [true, false]) {
    const { api, calls } = source();
    const plan = await runPublication(api, 'Example/repo', dryRun);
    assert.equal(plan.sha, sha);
    assert.equal(plan.dispatched, !dryRun);
    const writes = calls.filter(call => call.options.method === 'POST');
    assert.equal(writes.length, dryRun ? 0 : 1);
    if (!dryRun) assert.deepEqual(writes[0].options.body, { ref: 'canary' });
  }
});

test('complete matching publication and active publication skip; stale manifests retry', async () => {
  for (const [options, action] of [[{ complete: true }, 'skip'], [{ running: true }, 'skip'],
    [{ complete: true, badManifest: true }, 'dispatch']]) {
    const { api, calls } = source(options);
    const plan = await runPublication(api, 'Example/repo', false);
    assert.equal(plan.action, action);
    assert.equal(calls.filter(call => call.options.method === 'POST').length, action === 'dispatch' ? 1 : 0);
  }
});

test('API uncertainty fails instead of dispatching expensive work', async () => {
  const { api, calls } = source({ fail: true });
  await assert.rejects(runPublication(api, 'Example/repo', false), /HTTP 503/);
  assert.equal(calls.filter(call => call.options.method === 'POST').length, 0);
});

test('hourly publication replaces push and protects active builds', async () => {
  const workflow = await readFile(new URL('../../../.github/workflows/core-binaries-hourly.yml', import.meta.url), 'utf8');
  const publisher = await readFile(new URL('../../../.github/workflows/core-binaries.yml', import.meta.url), 'utf8');
  assert.match(workflow, /cron: '23 \* \* \* \*'/);
  assert.match(workflow, /ref: canary/);
  assert.match(workflow, /CORE_BINARIES_HOURLY_ENABLED == 'true'/);
  assert.match(workflow, /default: true/);
  assert.match(workflow, /cancel-in-progress: false/);
  assert.doesNotMatch(publisher, /^  push:/m);
  assert.match(publisher, /cancel-in-progress: \$\{\{ github.event_name == 'pull_request' \}\}/);
  assert.match(publisher, /workflow_dispatch:/);
});
