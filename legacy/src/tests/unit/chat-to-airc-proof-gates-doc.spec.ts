import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const repoRoot = resolve(__dirname, '../../..');
const proofGates = readFileSync(
  resolve(repoRoot, 'docs/grid/CHAT-TO-AIRC-MIGRATION-PROOF-GATES.md'),
  'utf8'
);
const inventory = readFileSync(
  resolve(repoRoot, 'docs/grid/generated/chat-to-airc-inventory.md'),
  'utf8'
);

const requiredInventoryPaths = [
  'src/commands/collaboration/chat/send/server/ChatSendServerCommand.ts',
  'src/commands/collaboration/chat/export/server/ChatExportServerCommand.ts',
  'src/commands/collaboration/chat/poll/server/ChatPollServerCommand.ts',
  'src/system/data/entities/ChatMessageEntity.ts',
  'src/system/user/server/PersonaUser.ts',
  'src/system/voice/server/VoiceWebSocketHandler.ts',
  'src/daemons/training-daemon/server/TrainingDaemonServer.ts',
  'src/system/sentinel/pipelines/*',
];

for (const path of requiredInventoryPaths) {
  assert.ok(
    inventory.includes(path),
    `chat-to-airc inventory must mention ${path}`
  );
}

const requiredAdapterTerms = [
  'typed adapter',
  'no raw SQL',
  'no local Postgres',
  'chat send latency',
  'persona reply roundtrip latency',
  'AIRC PR #638',
];

for (const term of requiredAdapterTerms) {
  assert.ok(
    inventory.includes(term) || proofGates.includes(term),
    `chat-to-airc docs must preserve migration gate term: ${term}`
  );
}

assert.ok(
  proofGates.includes('generated/chat-to-airc-inventory.md'),
  'proof gates must link to the generated inventory artifact'
);

assert.ok(
  proofGates.includes("Continuum must not bind to AIRC's SQLite tables directly."),
  'proof gates must keep Continuum behind AIRC typed APIs, not table coupling'
);

console.log('chat-to-airc proof gates docs: ok');
