#!/usr/bin/env tsx

import { generateUUID } from '@system/core/types/CrossPlatformUUID';
import { createAiKeyDiffParams, createAiKeyDiffResult } from '../../shared/AiKeyDiffTypes';

const context = { environment: 'server' as const };
const sessionId = generateUUID();
const params = createAiKeyDiffParams(context, sessionId, generateUUID(), {
  localEntries: [],
  remoteEntries: [],
  targetNode: 'windows-rtx',
});

if (!Array.isArray(params.localEntries) || !Array.isArray(params.remoteEntries)) {
  throw new Error('AiKeyDiff params factory did not preserve entry arrays');
}

const result = createAiKeyDiffResult(context, sessionId, {
  success: true,
});

if (!result.success || result.mergePlanId !== '' || result.actionCount !== 0 || result.conflictCount !== 0) {
  throw new Error('AiKeyDiff result factory did not apply defaults correctly');
}

console.log('AiKeyDiff integration smoke passed');
