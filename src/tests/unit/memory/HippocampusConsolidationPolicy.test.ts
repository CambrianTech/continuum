import { describe, it, expect, afterEach } from 'vitest';
import { getDefaultConsolidationMode, isLlmMemorySynthesisEnabled } from '../../../system/user/server/modules/cognitive/memory/HippocampusConsolidationPolicy';

const ENV_NAME = 'CONTINUUM_ENABLE_LLM_MEMORY_SYNTHESIS';
const originalValue = process.env[ENV_NAME];

describe('Hippocampus consolidation policy', () => {
  afterEach(() => {
    if (originalValue === undefined) {
      delete process.env[ENV_NAME];
    } else {
      process.env[ENV_NAME] = originalValue;
    }
  });

  it('uses raw consolidation by default so background memory cannot steal chat inference', () => {
    delete process.env[ENV_NAME];

    expect(getDefaultConsolidationMode()).toBe('raw');
    expect(isLlmMemorySynthesisEnabled()).toBe(false);
  });

  it('uses semantic compression only when explicitly enabled', () => {
    process.env[ENV_NAME] = '1';

    expect(getDefaultConsolidationMode()).toBe('semantic');
    expect(isLlmMemorySynthesisEnabled()).toBe(true);
  });
});
