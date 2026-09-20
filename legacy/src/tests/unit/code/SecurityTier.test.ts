/**
 * SecurityTier Unit Tests
 *
 * Tests the risk-based access control tier system:
 * - Tier definitions (discovery, read, write, system)
 * - Tier lookups and ordering
 * - Risk → tier mapping
 * - Risk → approval requirement mapping
 */

import { describe, it, expect } from 'vitest';
import {
  getTier,
  tierAtLeast,
  riskToTier,
  riskRequiresApproval,
  TIER_LEVELS,
  type SecurityTierLevel,
  type RiskLevel,
} from '../../../system/code/server/SecurityTier';

describe('SecurityTier', () => {
  describe('getTier()', () => {
    it('returns discovery tier', () => {
      const tier = getTier('discovery');
      expect(tier.level).toBe('discovery');
      expect(tier.allowProcessSpawn).toBe(false);
      expect(tier.allowNetworkAccess).toBe(false);
      expect(tier.requiresApproval).toBe(false);
      expect(tier.maxFileSizeBytes).toBe(0);
    });

    it('returns read tier', () => {
      const tier = getTier('read');
      expect(tier.level).toBe('read');
      expect(tier.allowProcessSpawn).toBe(false);
      expect(tier.maxFileSizeBytes).toBe(0);
    });

    it('returns write tier', () => {
      const tier = getTier('write');
      expect(tier.level).toBe('write');
      expect(tier.allowProcessSpawn).toBe(false);
      expect(tier.maxFileSizeBytes).toBeGreaterThan(0);
    });

    it('returns system tier', () => {
      const tier = getTier('system');
      expect(tier.level).toBe('system');
      expect(tier.allowProcessSpawn).toBe(true);
      expect(tier.allowNetworkAccess).toBe(true);
      expect(tier.requiresApproval).toBe(true);
    });
  });

  describe('tier allowlists', () => {
    it('discovery tier allows only read-type commands', () => {
      const tier = getTier('discovery');
      expect(tier.allowedCommands).toContain('code/tree');
      expect(tier.allowedCommands).toContain('code/search');
      expect(tier.allowedCommands).toContain('code/read');
      expect(tier.allowedCommands).toContain('code/history');
      expect(tier.allowedCommands).not.toContain('code/write');
      expect(tier.allowedCommands).not.toContain('code/edit');
    });

    it('discovery tier explicitly denies write and system commands', () => {
      const tier = getTier('discovery');
      expect(tier.deniedCommands).toContain('code/write');
      expect(tier.deniedCommands).toContain('code/edit');
      expect(tier.deniedCommands).toContain('development/*');
      expect(tier.deniedCommands).toContain('system/*');
    });

    it('read tier extends discovery with analysis commands', () => {
      const tier = getTier('read');
      expect(tier.allowedCommands).toContain('code/tree');
      expect(tier.allowedCommands).toContain('code/diff');
      expect(tier.allowedCommands).toContain('data/list');
      expect(tier.allowedCommands).toContain('data/read');
      expect(tier.allowedCommands).not.toContain('code/write');
    });

    it('write tier adds mutation commands', () => {
      const tier = getTier('write');
      expect(tier.allowedCommands).toContain('code/write');
      expect(tier.allowedCommands).toContain('code/edit');
      expect(tier.allowedCommands).toContain('code/undo');
    });

    it('write tier includes code/verify for build verification', () => {
      const tier = getTier('write');
      expect(tier.allowedCommands).toContain('code/verify');
    });

    it('write tier denies shell and system commands', () => {
      const tier = getTier('write');
      expect(tier.deniedCommands).toContain('development/exec');
      expect(tier.deniedCommands).toContain('development/sandbox-execute');
      expect(tier.deniedCommands).toContain('system/*');
    });

    it('system tier allows everything', () => {
      const tier = getTier('system');
      expect(tier.allowedCommands).toContain('*');
      expect(tier.deniedCommands).toEqual([]);
    });
  });

  describe('tier budgets', () => {
    it('discovery tier has moderate budget', () => {
      const tier = getTier('discovery');
      expect(tier.maxToolCalls).toBe(30);
      expect(tier.maxDurationMs).toBe(60_000);
    });

    it('write tier has tighter tool call budget', () => {
      const tier = getTier('write');
      expect(tier.maxToolCalls).toBe(20);
      expect(tier.maxDurationMs).toBe(120_000);
    });

    it('system tier has generous budget', () => {
      const tier = getTier('system');
      expect(tier.maxToolCalls).toBe(50);
      expect(tier.maxDurationMs).toBe(300_000);
    });
  });

  describe('TIER_LEVELS ordering', () => {
    it('lists tiers in ascending privilege order', () => {
      expect(TIER_LEVELS).toEqual(['discovery', 'read', 'write', 'system']);
    });
  });

  describe('tierAtLeast()', () => {
    it('same tier is at least itself', () => {
      for (const level of TIER_LEVELS) {
        expect(tierAtLeast(level, level)).toBe(true);
      }
    });

    it('system is at least every tier', () => {
      for (const level of TIER_LEVELS) {
        expect(tierAtLeast('system', level)).toBe(true);
      }
    });

    it('discovery is not at least write', () => {
      expect(tierAtLeast('discovery', 'write')).toBe(false);
    });

    it('write is at least read', () => {
      expect(tierAtLeast('write', 'read')).toBe(true);
    });

    it('read is not at least write', () => {
      expect(tierAtLeast('read', 'write')).toBe(false);
    });
  });

  describe('riskToTier()', () => {
    it('low risk maps to write tier', () => {
      expect(riskToTier('low')).toBe('write');
    });

    it('medium risk maps to write tier', () => {
      expect(riskToTier('medium')).toBe('write');
    });

    it('high risk maps to write tier (governance decides approval)', () => {
      expect(riskToTier('high')).toBe('write');
    });

    it('critical risk maps to system tier', () => {
      expect(riskToTier('critical')).toBe('system');
    });
  });

  describe('riskRequiresApproval()', () => {
    it('low risk single-agent does not require approval', () => {
      expect(riskRequiresApproval('low', false)).toBe(false);
    });

    it('medium risk single-agent does not require approval', () => {
      expect(riskRequiresApproval('medium', false)).toBe(false);
    });

    it('high risk single-agent requires approval', () => {
      expect(riskRequiresApproval('high', false)).toBe(true);
    });

    it('critical risk always requires approval', () => {
      expect(riskRequiresApproval('critical', false)).toBe(true);
      expect(riskRequiresApproval('critical', true)).toBe(true);
    });

    it('multi-agent always requires approval regardless of risk', () => {
      const risks: RiskLevel[] = ['low', 'medium', 'high', 'critical'];
      for (const risk of risks) {
        expect(riskRequiresApproval(risk, true)).toBe(true);
      }
    });
  });
});
