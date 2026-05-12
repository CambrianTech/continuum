import { describe, expect, it, afterEach } from 'vitest';
import { resolveInstallTier } from '../../server/seed-in-process';

const ORIGINAL_CONTINUUM_TIER = process.env.CONTINUUM_TIER;
const ORIGINAL_TIER = process.env.TIER;

afterEach(() => {
  process.env.CONTINUUM_TIER = ORIGINAL_CONTINUUM_TIER;
  process.env.TIER = ORIGINAL_TIER;
});

describe('seed install tier resolution', () => {
  it('uses CONTINUUM_TIER before host/container memory inference', () => {
    process.env.CONTINUUM_TIER = 'full';
    delete process.env.TIER;

    expect(resolveInstallTier()).toBe('full');
  });

  it('uses TIER when CONTINUUM_TIER is absent', () => {
    delete process.env.CONTINUUM_TIER;
    process.env.TIER = 'mid';

    expect(resolveInstallTier()).toBe('mid');
  });

  it('fails on invalid explicit tiers', () => {
    process.env.CONTINUUM_TIER = 'primary';
    delete process.env.TIER;

    expect(() => resolveInstallTier()).toThrow(/invalid CONTINUUM_TIER\/TIER 'primary'/);
  });
});
