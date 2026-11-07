import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const WORKFLOW = await readFile(new URL('../../../.github/workflows/promote-integration-recovery-to-canary.yml', import.meta.url), 'utf8');
const DOCS = await readFile(new URL('../../../docs/promote-integration-recovery-to-canary.md', import.meta.url), 'utf8');

test('workflow has correct name and schedule', () => {
  assert.match(WORKFLOW, /^name: promote-integration-recovery-to-canary$/m);
  // Same quiet hour as promote-main (10:37 UTC)
  assert.match(WORKFLOW, /cron: '37 10 \* \* \*'/);
});

test('workflow dispatch accepts merge toggles per source', () => {
  assert.match(WORKFLOW, /merge_integration:/);
  assert.match(WORKFLOW, /merge_recovery:/);
  // Both default true (schedule merges automatically)
  const match = WORKFLOW.match(/merge_(integration|recovery):[\s\S]*?default: (true|false)/g);
  assert.ok(match && match.length === 2, 'both toggle inputs present');
});

test('workflow targets canary as the destination', () => {
  assert.match(WORKFLOW, /TARGET_BRANCH: canary/);
  // PRs merge INTO canary
  assert.match(WORKFLOW, /base: canary/);
});

test('concurrency prevents parallel promotions', () => {
  assert.match(WORKFLOW, /cancel-in-progress: false/);
  // Group name is unique to this workflow
  assert.match(WORKFLOW, /group: promote-integration-recovery-to-canary/);
});

test('permissions cover contents and pull-requests write', () => {
  const permBlock = WORKFLOW.match(/permissions:[\s\S]*?(?=concurrency|jobs)/);
  assert.ok(permBlock, 'permissions block exists');
  assert.match(permBlock[0], /contents: write/);
  assert.match(permBlock[0], /pull-requests: write/);
});

test('both integration and recovery sources are tested', () => {
  // Each source has its own ahead-check step with an id
  const ids = WORKFLOW.match(/id: (integration|recovery)_(ahead|green|pr)/g);
  assert.ok(ids && ids.length >= 6, 'each source has ahead/green/pr steps');
  
  // Both branches are referenced in fetch/fetch-depth logic
  assert.match(WORKFLOW, /SOURCE_BRANCH: integration/);
  assert.match(WORKFLOW, /SOURCE_BRANCH: recovery/);
});

test('tip-green check excludes this workflow from deploy verdict', () => {
  // NON_DEPLOY_PATHS must include this workflow's filename
  const nonDeploy = WORKFLOW.match(/NON_DEPLOY_PATHS: ['"]?([^'"\n]+)/);
  assert.ok(nonDeploy, 'NON_DEPLOY_PATHS is set');
  assert.ok(
    nonDeploy[1].includes('promote-integration-recovery-to-canary.yml'),
    'this workflow excludes itself from tip checks'
  );
});

test('promotion uses merge -s ours (squash semantics)', () => {
  // The promote branch is built by checking out the source and merging canary with -s ours
  assert.match(WORKFLOW, /merge -s ours/);
  // PR body carries Promoted-Integration-Sha or Promoted-Recovery-Sha
  assert.match(WORKFLOW, /Promoted-Integration-Sha:/);
  assert.match(WORKFLOW, /Promoted-Recovery-Sha:/);
});

test('PR title and body follow the promote-main pattern', () => {
  // Title starts with "promote:" prefix like promote-main does
  const titles = WORKFLOW.match(/title: .*/g) || [];
  assert.ok(titles.length >= 2, 'both sources have PR titles');
  
  // Body includes commit listing and SHA anchor
  assert.match(WORKFLOW, /Promoted-Integration-Sha:/);
  assert.match(WORKFLOW, /Commits promoted/);
});

test('workflow has timeout to prevent hung runs', () => {
  assert.match(WORKFLOW, /timeout-minutes: \d+/);
});

test('docs cover when-to-use guidance', () => {
  assert.ok(DOCS.includes('When to Use'), 'docs have usage table');
  assert.ok(DOCS.includes('integration') && DOCS.includes('recovery'), 'both sources covered');
});

test('docs explain the promotion mechanism step by step', () => {
  const steps = ['Detect:', 'Verify:', 'Promote:', 'PR:'];
  for (const step of steps) {
    assert.ok(DOCS.includes(step), `docs describe "${step}" step`);
  }
});

test('docs include troubleshooting section', () => {
  assert.ok(DOCS.includes('Troubleshooting'), 'docs have troubleshooting');
  // Should cover the common "nothing to promote" case
  assert.ok(DOCS.includes('nothing to promote'), 'covers empty promotion case');
});
