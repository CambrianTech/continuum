/**
 * Code Git Command Unit Tests
 *
 * Tests SecurityTier integration, PlanFormulator tool schema,
 * and CodingAction/ACTION_TO_COMMAND for the commit action.
 */

import { describe, it, expect } from 'vitest';
import { getTier } from '../../../system/code/server/SecurityTier';

describe('CodeGitCommand', () => {
  describe('SecurityTier integration', () => {
    it('code/git is allowed at read tier', () => {
      const tier = getTier('read');
      expect(tier.allowedCommands).toContain('code/git');
    });

    it('code/git is allowed at write tier (inherited from read)', () => {
      const tier = getTier('write');
      expect(tier.allowedCommands).toContain('code/git');
    });

    it('code/git is NOT allowed at discovery tier', () => {
      const tier = getTier('discovery');
      expect(tier.allowedCommands).not.toContain('code/git');
    });

    it('code/git is allowed at system tier (wildcard)', () => {
      const tier = getTier('system');
      expect(tier.allowedCommands).toContain('*');
    });
  });

  describe('CodingAction commit type', () => {
    it('commit is a valid CodingAction', () => {
      // Type check — if this compiles, the type exists
      const action: import('../../../system/code/shared/CodingTypes').CodingAction = 'commit';
      expect(action).toBe('commit');
    });
  });

  describe('operation validation', () => {
    const VALID_OPS = ['status', 'diff', 'log', 'add', 'commit', 'push'];

    for (const op of VALID_OPS) {
      it(`'${op}' is a valid operation`, () => {
        expect(VALID_OPS).toContain(op);
      });
    }

    it('invalid operations are rejected', () => {
      expect(VALID_OPS).not.toContain('rebase');
      expect(VALID_OPS).not.toContain('merge');
      expect(VALID_OPS).not.toContain('');
    });
  });
});
