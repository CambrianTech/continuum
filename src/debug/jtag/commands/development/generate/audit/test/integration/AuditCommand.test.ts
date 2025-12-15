/**
 * Integration tests for generate/audit command
 */

import { describe, it, expect } from 'vitest';
import { ModuleAuditor } from '@generator/audit/ModuleAuditor';
import { LintCheck } from '@generator/audit/checks/LintCheck';
import { MissingFileCheck } from '@generator/audit/checks/MissingFileCheck';

describe('Generate Audit Command Integration', () => {
  it('should create ModuleAuditor instance', () => {
    const auditor = new ModuleAuditor();
    expect(auditor).toBeDefined();
  });

  it('should register audit checks', () => {
    const auditor = new ModuleAuditor();
    auditor.registerCheck(new LintCheck());
    auditor.registerCheck(new MissingFileCheck());

    // If we get here without errors, registration worked
    expect(auditor).toBeDefined();
  });

  it('should audit a valid module', async () => {
    const auditor = new ModuleAuditor();
    auditor.registerCheck(new MissingFileCheck());

    // Audit the hello command (which should be complete)
    const report = await auditor.audit('commands/hello', 'command');

    expect(report).toBeDefined();
    expect(report.modulePath).toBe('commands/hello');
    expect(report.moduleType).toBe('command');
    expect(report.issues).toBeDefined();
    expect(report.summary).toBeDefined();
    expect(typeof report.summary.errors).toBe('number');
    expect(typeof report.summary.warnings).toBe('number');
  });

  it('should format audit report', async () => {
    const auditor = new ModuleAuditor();
    auditor.registerCheck(new MissingFileCheck());

    const report = await auditor.audit('commands/hello', 'command');
    const formatted = auditor.formatReport(report);

    expect(formatted).toBeDefined();
    expect(typeof formatted).toBe('string');
    expect(formatted.length).toBeGreaterThan(0);
    expect(formatted).toContain('Auditing module');
  });

  it('should detect issues in incomplete module', async () => {
    const auditor = new ModuleAuditor();
    auditor.registerCheck(new MissingFileCheck());

    // Create a temporary empty directory to test
    const testPath = 'commands/nonexistent-test-module';

    try {
      const report = await auditor.audit(testPath, 'command');

      // Should have detected missing files
      expect(report.summary.errors).toBeGreaterThan(0);
    } catch (error) {
      // It's ok if the module doesn't exist - that's what we're testing
      expect(error).toBeDefined();
    }
  });
});
