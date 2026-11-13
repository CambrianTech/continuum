import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { platforms, runPublication } from '../core-binaries-hourly.mjs';

const sha = '0123456789abcdef0123456789abcdef01234567';
test('docs-only tip publishes its build key and resolution failure cannot dispatch', async () => {
  const key = 'a'.repeat(40);
  const { api, calls } = source();
  const plan = await runPublication(api, 'Example/repo', false, async tip => {
    assert.equal(tip, sha);
    return key;
  });
  assert.equal(plan.sha, key);
  assert.ok(calls.some(call => call.path.endsWith(`/releases/tags/canary-${key.slice(0, 12)}`)));
  assert.deepEqual(calls.find(call => call.options.method === 'POST').options.body,
    { ref: 'canary', inputs: { build_key: key } });
  const refused = source();
  await assert.rejects(runPublication(refused.api, 'Example/repo', false,
    async () => { throw new Error('unreadable history'); }), /unreadable history/);
  assert.equal(refused.calls.filter(call => call.options.method === 'POST').length, 0);
});
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
    const plan = await runPublication(api, 'Example/repo', dryRun, async () => sha);
    assert.equal(plan.sha, sha);
    assert.equal(plan.dispatched, !dryRun);
    const writes = calls.filter(call => call.options.method === 'POST');
    assert.equal(writes.length, dryRun ? 0 : 1);
    if (!dryRun) assert.deepEqual(writes[0].options.body, { ref: 'canary', inputs: { build_key: sha } });
  }
});

test('complete matching publication and active publication skip; stale manifests retry', async () => {
  for (const [options, action] of [[{ complete: true }, 'skip'], [{ running: true }, 'skip'],
    [{ complete: true, badManifest: true }, 'dispatch']]) {
    const { api, calls } = source(options);
    const plan = await runPublication(api, 'Example/repo', false, async () => sha);
    assert.equal(plan.action, action);
    assert.equal(calls.filter(call => call.options.method === 'POST').length, action === 'dispatch' ? 1 : 0);
  }
});

test('API uncertainty fails instead of dispatching expensive work', async () => {
  const { api, calls } = source({ fail: true });
  await assert.rejects(runPublication(api, 'Example/repo', false, async () => sha), /HTTP 503/);
  assert.equal(calls.filter(call => call.options.method === 'POST').length, 0);
});

test('a completion wake drains a newer key skipped behind a build, then stops at its published manifests', async () => {
  const busy = source({ running: true });
  assert.equal((await runPublication(busy.api, 'Example/repo', false, async () => sha)).dispatched, false);
  assert.equal(busy.calls.filter(call => call.options.method === 'POST').length, 0);
  const finished = source();
  assert.equal((await runPublication(finished.api, 'Example/repo', false, async () => sha)).dispatched, true);
  assert.equal(finished.calls.filter(call => call.options.method === 'POST').length, 1);
  const published = source({ complete: true });
  assert.equal((await runPublication(published.api, 'Example/repo', false, async () => sha)).dispatched, false);
  assert.equal(published.calls.filter(call => call.options.method === 'POST').length, 0);
});

test('cron, successful canary checks and publication completion share admission and protect active builds', async () => {
  const workflow = await readFile(new URL('../../../.github/workflows/core-binaries-hourly.yml', import.meta.url), 'utf8');
  const publisher = await readFile(new URL('../../../.github/workflows/core-binaries.yml', import.meta.url), 'utf8');
  assert.match(workflow, /cron: '23 \* \* \* \*'/);
  assert.match(workflow, /workflow_run:\r?\n    workflows: \['Continuum Rust Tests'\]\r?\n    types: \[completed\]\r?\n    branches: \[canary\]/);
  assert.match(workflow, /workflow_run\.conclusion == 'success'/);
  assert.match(workflow, /workflow_run\.event == 'push'/);
  assert.match(workflow, /workflow_run\.name == 'Continuum Rust Tests' &&\s+github\.event\.workflow_run\.event == 'push'/);
  assert.doesNotMatch(workflow, /workflow_run\.name == 'core-binaries'/);
  assert.match(publisher, /needs: \[build, publish-check, publish-script-test, prune\]/);
  assert.match(publisher, /success\(\) && github.event_name == 'workflow_dispatch'/);
  assert.match(publisher, /group: core-binaries-hourly-dispatch/);
  assert.match(publisher, /PUBLICATION_COMPLETION_KEY: \$\{\{ inputs.build_key \}\}/);
  assert.match(workflow, /workflow_run\.head_branch == 'canary'/);
  assert.match(workflow, /workflow_run\.head_repository\.full_name == github\.repository/);

  assert.match(workflow, /ref: canary/);
  assert.match(workflow, /CORE_BINARIES_HOURLY_ENABLED == 'true'/);
  assert.match(workflow, /default: true/);
  assert.match(workflow, /cancel-in-progress: false/);
  assert.doesNotMatch(publisher, /^  push:/m);
  assert.match(publisher, /cancel-in-progress: \$\{\{ github.event_name == 'pull_request' \}\}/);
  assert.match(publisher, /workflow_dispatch:/);
  assert.match(publisher, /git checkout --detach "\$BUILD_KEY"/);
  assert.match(publisher, /GITHUB_SHA="\$BUILD_SHA" bash/);
  assert.match(publisher, /GITHUB_SHA="\$BUILD_KEY" bash/);
});

const oldKey = 'b'.repeat(40);
const completion = { runId: 17, attempt: 2, key: oldKey };
function completing({ runPatch = {}, partial = false, sameKey = false, others = [], failReceipt = false, badManifest = false } = {}) {
  const live = source();
  const published = source({ complete: true });
  const calls = [];
  const self = { id: 17, run_attempt: 2, event: 'workflow_dispatch', status: 'in_progress',
    path: '.github/workflows/core-binaries.yml', head_branch: 'canary',
    repository: { full_name: 'Example/repo' }, head_repository: { full_name: 'Example/repo' }, ...runPatch };
  const api = async (path, options = {}) => {
    calls.push({ path, options });
    if (path.endsWith('/actions/runs/17')) {
      if (failReceipt) throw new Error('receipt unavailable');
      return self;
    }
    if (path.includes('/runs?')) return { workflow_runs: [self, ...others] };
    if (path.endsWith(`/releases/tags/canary-${oldKey.slice(0, 12)}`)) {
      return partial ? null : published.api(path, options);
    }
    if (path.includes('/releases/assets/')) {
      const manifest = await published.api(path, options);
      return { ...manifest, git_sha: badManifest ? sha : oldKey };
    }
    return live.api(path, options);
  };
  return { calls, execute: (receipt = completion) =>
    runPublication(api, 'Example/repo', false, async () => sameKey ? oldKey : sha, receipt) };
}

test('publisher handoff admits a newer key once, and stops at a complete same key', async () => {
  const next = completing();
  assert.equal((await next.execute()).dispatched, true);
  assert.equal(next.calls.filter(c => c.options.method === 'POST').length, 1);
  assert.deepEqual(next.calls.find(c => c.options.method === 'POST').options.body,
    { ref: 'canary', inputs: { build_key: sha } });
  assert.equal((await completing({ sameKey: true }).execute()).dispatched, false);
  for (const status of ['queued', 'in_progress', 'waiting']) {
    const other = completing({ others: [{ id: 18, event: 'workflow_dispatch', status }] });
    assert.equal((await other.execute()).dispatched, false);
    assert.equal(other.calls.filter(c => c.options.method === 'POST').length, 0);
  }
});

test('handoff refuses partial publication, stale attempt, foreign run and uncertain receipt', async () => {
  for (const options of [{ partial: true }, { failReceipt: true }, { badManifest: true },
    ...[{ id: 18 }, { run_attempt: 1 }, { event: 'pull_request' }, { status: 'completed' },
      { path: '.github/workflows/unrelated.yml' }, { head_branch: 'feature' },
      { repository: { full_name: 'Foreign/repo' } }, { head_repository: null }].map(runPatch => ({ runPatch }))]) {
    const invalid = completing(options);
    await assert.rejects(invalid.execute());
    assert.equal(invalid.calls.filter(c => c.options.method === 'POST').length, 0);
  }
  for (const receipt of [{ ...completion, runId: NaN }, { ...completion, attempt: 0 },
    { ...completion, key: 'short' }]) {
    const invalid = completing();
    await assert.rejects(invalid.execute(receipt), /malformed/);
    assert.equal(invalid.calls.length, 0);
  }
});
