/**
 * ToolAllowlistEnforcer Unit Tests
 *
 * Tests the per-tier tool filtering gateway:
 * - Denied commands always blocked
 * - Allowed commands checked via glob matching
 * - Process spawn restrictions
 * - File size limits for write operations
 * - Audit logging
 * - Throwing vs non-throwing check modes
 */

import { describe, it, expect } from 'vitest';
import { ToolAllowlistEnforcer, ToolDeniedError } from '../../../system/code/server/ToolAllowlistEnforcer';
import { getTier } from '../../../system/code/server/SecurityTier';
import type { SecurityTier } from '../../../system/code/server/SecurityTier';

describe('ToolAllowlistEnforcer', () => {
  describe('discovery tier', () => {
    const enforcer = new ToolAllowlistEnforcer(getTier('discovery'));

    it('allows code/read', () => {
      expect(() => enforcer.enforce('code/read')).not.toThrow();
    });

    it('allows code/tree', () => {
      expect(() => enforcer.enforce('code/tree')).not.toThrow();
    });

    it('allows code/search', () => {
      expect(() => enforcer.enforce('code/search')).not.toThrow();
    });

    it('allows code/history', () => {
      expect(() => enforcer.enforce('code/history')).not.toThrow();
    });

    it('blocks code/write (explicit deny)', () => {
      expect(() => enforcer.enforce('code/write')).toThrow(ToolDeniedError);
    });

    it('blocks code/edit (explicit deny)', () => {
      expect(() => enforcer.enforce('code/edit')).toThrow(ToolDeniedError);
    });

    it('blocks development/* (glob deny)', () => {
      expect(() => enforcer.enforce('development/exec')).toThrow(ToolDeniedError);
      expect(() => enforcer.enforce('development/sandbox-execute')).toThrow(ToolDeniedError);
    });

    it('blocks system/* (glob deny)', () => {
      expect(() => enforcer.enforce('system/anything')).toThrow(ToolDeniedError);
    });

    it('blocks unknown commands not in allowlist', () => {
      expect(() => enforcer.enforce('data/list')).toThrow(ToolDeniedError);
    });
  });

  describe('read tier', () => {
    const enforcer = new ToolAllowlistEnforcer(getTier('read'));

    it('allows discovery commands', () => {
      expect(() => enforcer.enforce('code/read')).not.toThrow();
      expect(() => enforcer.enforce('code/tree')).not.toThrow();
    });

    it('allows data/list and data/read', () => {
      expect(() => enforcer.enforce('data/list')).not.toThrow();
      expect(() => enforcer.enforce('data/read')).not.toThrow();
    });

    it('allows code/diff', () => {
      expect(() => enforcer.enforce('code/diff')).not.toThrow();
    });

    it('blocks code/write', () => {
      expect(() => enforcer.enforce('code/write')).toThrow(ToolDeniedError);
    });
  });

  describe('write tier', () => {
    const enforcer = new ToolAllowlistEnforcer(getTier('write'));

    it('allows read + write commands', () => {
      expect(() => enforcer.enforce('code/read')).not.toThrow();
      expect(() => enforcer.enforce('code/write')).not.toThrow();
      expect(() => enforcer.enforce('code/edit')).not.toThrow();
      expect(() => enforcer.enforce('code/undo')).not.toThrow();
    });

    it('blocks development/exec (explicit deny)', () => {
      expect(() => enforcer.enforce('development/exec')).toThrow(ToolDeniedError);
    });

    it('blocks development/sandbox-execute (explicit deny)', () => {
      expect(() => enforcer.enforce('development/sandbox-execute')).toThrow(ToolDeniedError);
    });

    it('blocks system/* commands', () => {
      expect(() => enforcer.enforce('system/shell')).toThrow(ToolDeniedError);
    });
  });

  describe('system tier', () => {
    const enforcer = new ToolAllowlistEnforcer(getTier('system'));

    it('allows everything (wildcard)', () => {
      expect(() => enforcer.enforce('code/read')).not.toThrow();
      expect(() => enforcer.enforce('code/write')).not.toThrow();
      expect(() => enforcer.enforce('development/exec')).not.toThrow();
      expect(() => enforcer.enforce('system/anything')).not.toThrow();
      expect(() => enforcer.enforce('whatever/command')).not.toThrow();
    });
  });

  describe('file size enforcement', () => {
    it('write tier blocks oversized writes', () => {
      const enforcer = new ToolAllowlistEnforcer(getTier('write'));
      const oversizedContent = 'x'.repeat(2_000_000); // 2MB > 1MB limit

      const result = enforcer.check('code/write', { content: oversizedContent });
      expect(result.allowed).toBe(false);
      expect(result.reason).toContain('exceeds tier limit');
    });

    it('write tier allows content within size limit', () => {
      const enforcer = new ToolAllowlistEnforcer(getTier('write'));
      const content = 'x'.repeat(1000);

      const result = enforcer.check('code/write', { content });
      expect(result.allowed).toBe(true);
    });

    it('code/edit also checks file size', () => {
      const enforcer = new ToolAllowlistEnforcer(getTier('write'));
      const oversizedContent = 'x'.repeat(2_000_000);

      const result = enforcer.check('code/edit', { content: oversizedContent });
      expect(result.allowed).toBe(false);
    });

    it('discovery tier skips size check (no writes allowed anyway)', () => {
      const enforcer = new ToolAllowlistEnforcer(getTier('discovery'));
      // code/write is denied in discovery, so even a small write is blocked
      const result = enforcer.check('code/write', { content: 'small' });
      expect(result.allowed).toBe(false);
      expect(result.reason).toContain('denied');
    });
  });

  describe('process spawn restriction', () => {
    it('write tier blocks process spawn commands', () => {
      const enforcer = new ToolAllowlistEnforcer(getTier('write'));
      // development/exec is already in denied list for write tier, but also checked via allowProcessSpawn
      const result = enforcer.check('development/exec');
      expect(result.allowed).toBe(false);
    });

    it('system tier allows process spawn', () => {
      const enforcer = new ToolAllowlistEnforcer(getTier('system'));
      const result = enforcer.check('development/exec');
      expect(result.allowed).toBe(true);
    });
  });

  describe('check() (non-throwing)', () => {
    const enforcer = new ToolAllowlistEnforcer(getTier('discovery'));

    it('returns allowed=true for permitted commands', () => {
      const result = enforcer.check('code/read');
      expect(result.allowed).toBe(true);
      expect(result.toolName).toBe('code/read');
      expect(result.tierLevel).toBe('discovery');
    });

    it('returns allowed=false for denied commands', () => {
      const result = enforcer.check('code/write');
      expect(result.allowed).toBe(false);
      expect(result.toolName).toBe('code/write');
      expect(result.tierLevel).toBe('discovery');
      expect(result.reason).toBeTruthy();
    });
  });

  describe('audit log', () => {
    it('records every enforce() call', () => {
      const enforcer = new ToolAllowlistEnforcer(getTier('write'));

      enforcer.enforce('code/read');
      enforcer.enforce('code/write');
      try { enforcer.enforce('development/exec'); } catch { /* expected */ }

      expect(enforcer.auditLog).toHaveLength(3);
      expect(enforcer.auditLog[0].allowed).toBe(true);
      expect(enforcer.auditLog[0].toolName).toBe('code/read');
      expect(enforcer.auditLog[1].allowed).toBe(true);
      expect(enforcer.auditLog[1].toolName).toBe('code/write');
      expect(enforcer.auditLog[2].allowed).toBe(false);
      expect(enforcer.auditLog[2].toolName).toBe('development/exec');
    });

    it('check() does NOT record to audit log', () => {
      const enforcer = new ToolAllowlistEnforcer(getTier('discovery'));

      enforcer.check('code/read');
      enforcer.check('code/write');

      expect(enforcer.auditLog).toHaveLength(0);
    });
  });

  describe('ToolDeniedError', () => {
    it('has correct properties', () => {
      const enforcer = new ToolAllowlistEnforcer(getTier('discovery'));

      try {
        enforcer.enforce('code/write');
        expect.unreachable('Should have thrown');
      } catch (error) {
        expect(error).toBeInstanceOf(ToolDeniedError);
        const denied = error as ToolDeniedError;
        expect(denied.toolName).toBe('code/write');
        expect(denied.tierLevel).toBe('discovery');
        expect(denied.message).toContain('code/write');
        expect(denied.message).toContain('denied');
      }
    });
  });

  describe('glob matching', () => {
    it('exact match works', () => {
      const enforcer = new ToolAllowlistEnforcer(getTier('discovery'));
      const result = enforcer.check('code/read');
      expect(result.allowed).toBe(true);
    });

    it('wildcard * matches everything', () => {
      const enforcer = new ToolAllowlistEnforcer(getTier('system'));
      const result = enforcer.check('literally/anything');
      expect(result.allowed).toBe(true);
    });

    it('prefix/* matches prefix/anything', () => {
      // discovery tier denies development/*
      const enforcer = new ToolAllowlistEnforcer(getTier('discovery'));
      expect(enforcer.check('development/exec').allowed).toBe(false);
      expect(enforcer.check('development/build').allowed).toBe(false);
      expect(enforcer.check('development/sandbox-execute').allowed).toBe(false);
    });

    it('prefix/* does not match the prefix itself', () => {
      // Create a custom tier for testing
      const customTier: SecurityTier = {
        level: 'write',
        allowedCommands: ['code/*'],
        deniedCommands: [],
        maxToolCalls: 10,
        maxDurationMs: 60_000,
        maxFileSizeBytes: 0,
        allowProcessSpawn: false,
        allowNetworkAccess: false,
        requiresApproval: false,
      };
      const enforcer = new ToolAllowlistEnforcer(customTier);

      // 'code/*' should match 'code/read' but NOT 'code' itself
      expect(enforcer.check('code/read').allowed).toBe(true);
      expect(enforcer.check('code').allowed).toBe(false);
    });
  });

  describe('tier property access', () => {
    it('exposes the tier', () => {
      const tier = getTier('write');
      const enforcer = new ToolAllowlistEnforcer(tier);
      expect(enforcer.tier).toBe(tier);
      expect(enforcer.tier.level).toBe('write');
    });
  });
});
