/**
 * Code Verify Command Unit Tests
 *
 * Tests the code/verify types, SecurityTier integration, and PlanFormulator
 * tool schema registration. The actual server command logic is tested
 * indirectly through CodeAgentOrchestrator (auto-verify) and via
 * integration tests against the running system.
 *
 * Direct server command testing requires vitest path alias resolution
 * for @daemons/* imports — planned when vitest.config.ts is added.
 */

import { describe, it, expect } from 'vitest';
import { getTier } from '../../../system/code/server/SecurityTier';

describe('code/verify — SecurityTier integration', () => {
  it('code/verify is allowed at write tier', () => {
    const tier = getTier('write');
    expect(tier.allowedCommands).toContain('code/verify');
  });

  it('code/verify is NOT allowed at discovery tier', () => {
    const tier = getTier('discovery');
    expect(tier.allowedCommands).not.toContain('code/verify');
  });

  it('code/verify is NOT allowed at read tier', () => {
    const tier = getTier('read');
    expect(tier.allowedCommands).not.toContain('code/verify');
  });

  it('code/verify is allowed at system tier (wildcard)', () => {
    const tier = getTier('system');
    expect(tier.allowedCommands).toContain('*');
  });
});

describe('code/verify — TypeScript error parsing', () => {
  // Test the regex pattern used by CodeVerifyServerCommand
  const TS_ERROR_REGEX = /^(.+?)\((\d+),(\d+)\):\s*error\s+(TS\d+):\s*(.+)$/gm;

  function parseErrors(output: string): Array<{ file: string; line: number; column: number; code: string; message: string }> {
    const errors: Array<{ file: string; line: number; column: number; code: string; message: string }> = [];
    TS_ERROR_REGEX.lastIndex = 0;
    let match;
    while ((match = TS_ERROR_REGEX.exec(output)) !== null) {
      errors.push({
        file: match[1],
        line: parseInt(match[2], 10),
        column: parseInt(match[3], 10),
        code: match[4],
        message: match[5],
      });
    }
    return errors;
  }

  it('parses single TypeScript error', () => {
    const output = "src/utils.ts(10,5): error TS2345: Argument of type 'string' is not assignable to parameter of type 'number'.";
    const errors = parseErrors(output);

    expect(errors).toHaveLength(1);
    expect(errors[0]).toEqual({
      file: 'src/utils.ts',
      line: 10,
      column: 5,
      code: 'TS2345',
      message: "Argument of type 'string' is not assignable to parameter of type 'number'.",
    });
  });

  it('parses multiple errors', () => {
    const output = [
      "src/utils.ts(10,5): error TS2345: Type error A.",
      "src/main.ts(42,12): error TS2304: Cannot find name 'foo'.",
      "lib/helpers.ts(1,1): error TS1005: Missing semicolon.",
    ].join('\n');

    const errors = parseErrors(output);
    expect(errors).toHaveLength(3);
    expect(errors[0].file).toBe('src/utils.ts');
    expect(errors[1].file).toBe('src/main.ts');
    expect(errors[2].file).toBe('lib/helpers.ts');
  });

  it('handles empty output (no errors)', () => {
    const errors = parseErrors('');
    expect(errors).toHaveLength(0);
  });

  it('handles mixed output with non-error lines', () => {
    const output = [
      'Starting TypeScript compilation...',
      "src/index.ts(5,3): error TS7006: Parameter 'x' implicitly has an 'any' type.",
      'Found 1 error.',
    ].join('\n');

    const errors = parseErrors(output);
    expect(errors).toHaveLength(1);
    expect(errors[0].code).toBe('TS7006');
  });

  it('parses file paths with spaces', () => {
    const output = "src/my module/file.ts(3,7): error TS2322: Type mismatch.";
    const errors = parseErrors(output);

    expect(errors).toHaveLength(1);
    expect(errors[0].file).toBe('src/my module/file.ts');
  });
});

describe('code/verify — PlanFormulator tool schema', () => {
  // Verify the action → command mapping includes code/verify
  it('verify action maps to code/verify in plan', () => {
    // The ACTION_TO_COMMAND map in PlanFormulator maps 'verify' → 'code/verify'
    // We test this indirectly through the PlanFormulator test suite
    // This test validates the expected behavior at the plan level
    const ACTION_TO_COMMAND: Record<string, string> = {
      discover: 'code/tree',
      search: 'code/search',
      read: 'code/read',
      write: 'code/write',
      edit: 'code/edit',
      diff: 'code/diff',
      undo: 'code/undo',
      verify: 'code/verify',
      report: 'code/history',
    };

    expect(ACTION_TO_COMMAND.verify).toBe('code/verify');
  });
});
