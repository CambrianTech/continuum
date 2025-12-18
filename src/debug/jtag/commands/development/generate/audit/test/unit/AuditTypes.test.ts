/**
 * Unit tests for Audit System Types
 */

import { describe, it, expect } from 'vitest';
import type {
  ModuleType,
  IssueSeverity,
  Issue,
  AuditReport,
} from '@shared/GenerateAuditTypes';

describe('Audit Type Definitions', () => {
  it('should have valid ModuleType values', () => {
    const validTypes: ModuleType[] = ['command', 'widget', 'daemon'];
    expect(validTypes).toContain('command');
    expect(validTypes).toContain('widget');
    expect(validTypes).toContain('daemon');
  });

  it('should have valid IssueSeverity values', () => {
    const validSeverities: IssueSeverity[] = ['error', 'warning', 'info'];
    expect(validSeverities).toContain('error');
    expect(validSeverities).toContain('warning');
    expect(validSeverities).toContain('info');
  });

  it('should create valid Issue object', () => {
    const issue: Issue = {
      severity: 'error',
      category: 'lint',
      message: 'Test issue',
      filePath: 'test.ts',
      lineNumber: 42,
      fixable: true,
      suggestedFix: 'Fix the code',
    };

    expect(issue.severity).toBe('error');
    expect(issue.category).toBe('lint');
    expect(issue.message).toBe('Test issue');
    expect(issue.fixable).toBe(true);
  });

  it('should create valid AuditReport object', () => {
    const report: AuditReport = {
      modulePath: 'commands/test',
      moduleType: 'command',
      timestamp: Date.now(),
      issues: [],
      summary: {
        errors: 0,
        warnings: 0,
        fixable: 0,
      },
    };

    expect(report.moduleType).toBe('command');
    expect(report.issues).toHaveLength(0);
    expect(report.summary.errors).toBe(0);
  });

  it('should properly categorize issue severity', () => {
    const errorIssue: Issue = {
      severity: 'error',
      category: 'lint',
      message: 'Critical error',
      fixable: false,
    };

    const warningIssue: Issue = {
      severity: 'warning',
      category: 'readme',
      message: 'Minor warning',
      fixable: true,
    };

    expect(errorIssue.severity).toBe('error');
    expect(warningIssue.severity).toBe('warning');
    expect(errorIssue.fixable).toBe(false);
    expect(warningIssue.fixable).toBe(true);
  });
});
