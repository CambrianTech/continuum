#!/usr/bin/env tsx

import { generateUUID } from '@system/core/types/CrossPlatformUUID';
import { createAiKeyStatusResult } from '../../shared/AiKeyStatusTypes';

const context = { environment: 'server' as const };
const sessionId = generateUUID();
const result = createAiKeyStatusResult(context, sessionId, {
  success: true,
  configuredCount: 0,
  totalCount: 0
});

if (!result.success || result.entries.length !== 0 || result.totalCount !== 0) {
  throw new Error('AiKeyStatus result factory did not apply defaults correctly');
}

console.log('AiKeyStatus integration smoke passed');
