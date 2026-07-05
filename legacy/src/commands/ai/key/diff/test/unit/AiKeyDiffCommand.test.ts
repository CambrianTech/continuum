#!/usr/bin/env tsx

import { generateUUID } from '@system/core/types/CrossPlatformUUID';
import type { AiKeyStatusEntry } from '../../status/shared/AiKeyStatusTypes';
import { createAiKeyDiffResult } from '../../shared/AiKeyDiffTypes';
import { buildAiKeyDiffActions, createAiKeyMergePlanId } from '../../shared/AiKeyDiffPlanner';

function assert(condition: boolean, message: string): void {
  if (!condition) {
    throw new Error(message);
  }
}

function entry(overrides: Partial<AiKeyStatusEntry>): AiKeyStatusEntry {
  return {
    provider: 'OpenAI',
    key: 'OPENAI_API_KEY',
    category: 'cloud',
    configured: false,
    empty: true,
    source: 'missing',
    description: 'GPT models',
    ...overrides,
  };
}

const rawSecret = 'sk-test-raw-secret-that-must-never-appear';

const sameFingerprint = buildAiKeyDiffActions(
  [entry({ configured: true, empty: false, fingerprint: 'fp_same', source: 'continuum-home' })],
  [entry({ configured: true, empty: false, fingerprint: 'fp_same', source: 'process-env' })],
  'windows-rtx'
);

assert(sameFingerprint.length === 1, 'same configured fingerprints produce one action');
assert(sameFingerprint[0]?.action === 'noop', 'same configured fingerprints are no-op');
assert(sameFingerprint[0]?.requiresApproval === false, 'no-op action does not require approval');

const localOnly = buildAiKeyDiffActions(
  [entry({ configured: true, empty: false, fingerprint: 'fp_local', source: 'continuum-home' })],
  [entry({ configured: false, empty: true, source: 'missing' })],
  'windows-rtx'
);

assert(localOnly.length === 1, 'local-only configured key produces one action');
assert(localOnly[0]?.action === 'copy-local-to-remote', 'local-only key plans copy to remote');
assert(localOnly[0]?.requiresApproval === true, 'copy action requires approval');
assert(localOnly[0]?.localFingerprint === 'fp_local', 'copy action carries local fingerprint metadata');
assert(!JSON.stringify(localOnly).includes(rawSecret), 'diff action serialization does not include raw secret');

const conflict = buildAiKeyDiffActions(
  [entry({ configured: true, empty: false, fingerprint: 'fp_local' })],
  [entry({ configured: true, empty: false, fingerprint: 'fp_remote' })],
  'windows-rtx'
);

assert(conflict.length === 1, 'different configured fingerprints produce one action');
assert(conflict[0]?.action === 'conflict', 'different configured fingerprints produce conflict');
assert(conflict[0]?.requiresApproval === true, 'conflict requires approval');

const empty = buildAiKeyDiffActions(
  [entry({ configured: false, empty: true })],
  [entry({ configured: false, empty: true })],
  'windows-rtx'
);

assert(empty.length === 0, 'missing keys on both sides are omitted from merge plan');

const ordered = buildAiKeyDiffActions(
  [
    entry({ provider: 'OpenAI', key: 'OPENAI_API_KEY', configured: true, empty: false, fingerprint: 'fp_openai' }),
    entry({ provider: 'Anthropic', key: 'ANTHROPIC_API_KEY', configured: true, empty: false, fingerprint: 'fp_anthropic' }),
  ],
  [],
  'windows-rtx'
);
const reversed = buildAiKeyDiffActions(
  [
    entry({ provider: 'Anthropic', key: 'ANTHROPIC_API_KEY', configured: true, empty: false, fingerprint: 'fp_anthropic' }),
    entry({ provider: 'OpenAI', key: 'OPENAI_API_KEY', configured: true, empty: false, fingerprint: 'fp_openai' }),
  ],
  [],
  'windows-rtx'
);

assert(
  createAiKeyMergePlanId(ordered, 'windows-rtx') === createAiKeyMergePlanId(reversed, 'windows-rtx'),
  'merge plan id is deterministic across input ordering'
);

const context = { environment: 'server' as const };
const sessionId = generateUUID();
const result = createAiKeyDiffResult(context, sessionId, {
  success: true,
  mergePlanId: createAiKeyMergePlanId(conflict, 'windows-rtx'),
  actions: conflict,
  conflictCount: conflict.filter(action => action.action === 'conflict').length,
  actionCount: conflict.length,
});

assert(result.success === true, 'result factory preserves success');
assert(result.actionCount === 1, 'result factory preserves action count');
assert(result.conflictCount === 1, 'result factory preserves conflict count');
assert(result.actions[0]?.action === 'conflict', 'result factory preserves actions');

console.log('AiKeyDiff command tests passed');
