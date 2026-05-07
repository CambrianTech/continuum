import { describe, expect, it } from 'vitest';
import { shouldInitializeCodebaseIndexing } from '../../system/core/system/server/ServiceInitializer';

describe('ServiceInitializer', () => {
  describe('shouldInitializeCodebaseIndexing', () => {
    it('keeps codebase indexing off by default during development startup', () => {
      expect(shouldInitializeCodebaseIndexing({}, 'development')).toBe(false);
    });

    it('allows explicit opt-in outside production', () => {
      expect(shouldInitializeCodebaseIndexing({ CONTINUUM_ENABLE_CODEBASE_INDEX: '1' }, 'development')).toBe(true);
      expect(shouldInitializeCodebaseIndexing({ CONTINUUM_ENABLE_CODEBASE_INDEX: 'true' }, 'test')).toBe(true);
    });

    it('lets skip override opt-in', () => {
      expect(shouldInitializeCodebaseIndexing({
        CONTINUUM_ENABLE_CODEBASE_INDEX: '1',
        SKIP_CODEBASE_INDEX: '1',
      }, 'development')).toBe(false);
    });

    it('never auto-indexes in production startup', () => {
      expect(shouldInitializeCodebaseIndexing({ CONTINUUM_ENABLE_CODEBASE_INDEX: '1' }, 'production')).toBe(false);
    });
  });
});
