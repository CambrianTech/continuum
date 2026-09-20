#!/usr/bin/env tsx

import { generateUUID } from '@system/core/types/CrossPlatformUUID';
import { createAiKeyStatusResult } from '../../shared/AiKeyStatusTypes';
import { createAiKeyStatusEntry, fingerprintAiKey } from '../../shared/AiKeyStatusRedaction';

function assert(condition: boolean, message: string): void {
  if (!condition) {
    throw new Error(message);
  }
}

const secret = 'sk-test-secret-value-1234567890';
const fingerprint = fingerprintAiKey('OPENAI_API_KEY', secret);

assert(fingerprint !== undefined, 'non-empty values produce fingerprints');
assert(fingerprint !== secret, 'fingerprint is not the secret value');
assert(!fingerprint?.includes('sk-test'), 'fingerprint does not include key prefix');

const entry = createAiKeyStatusEntry({
  provider: 'OpenAI',
  key: 'OPENAI_API_KEY',
  category: 'cloud',
  description: 'GPT models',
  value: secret
});

const serialized = JSON.stringify(entry);

assert(entry.configured === true, 'configured is true for non-empty keys');
assert(entry.empty === false, 'empty is false for non-empty keys');
assert(entry.source === 'continuum-home', 'home config wins as source');
assert(!serialized.includes(secret), 'status entry never serializes raw secret');
assert(!serialized.includes(secret.slice(0, 7)), 'status entry never serializes masked prefix');
assert(!serialized.includes(secret.slice(-4)), 'status entry never serializes masked suffix');

const emptyEntry = createAiKeyStatusEntry({
  provider: 'OpenAI',
  key: 'OPENAI_API_KEY',
  category: 'cloud',
  description: 'GPT models',
  value: ''
});

assert(emptyEntry.configured === false, 'empty values are not configured');
assert(emptyEntry.fingerprint === undefined, 'empty values have no fingerprint');

const context = { environment: 'server' as const };
const sessionId = generateUUID();
const result = createAiKeyStatusResult(context, sessionId, {
  success: true,
  entries: [entry],
  configuredCount: 1,
  totalCount: 1
});

assert(result.success === true, 'result factory preserves success');
assert(result.entries.length === 1, 'result factory preserves entries');
assert(result.configuredCount === 1, 'result factory preserves configured count');

console.log('AiKeyStatus command tests passed');
