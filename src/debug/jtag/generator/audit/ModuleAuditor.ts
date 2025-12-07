/**
 * ModuleAuditor - Core audit system for modules
 *
 * Runs health checks on modules and applies fixes automatically.
 */

import type {
  AuditReport,
  AuditOptions,
  IAuditCheck,
  Issue,
  ModuleType,
} from './AuditTypes';

export class ModuleAuditor {
  private checks: IAuditCheck[] = [];

  constructor() {
    // Checks will be registered here as they're implemented
  }

  /**
   * Register an audit check
   */
  registerCheck(check: IAuditCheck): void {
    this.checks.push(check);
  }

  /**
   * Audit a module and return issues found
   */
  async audit(
    modulePath: string,
    moduleType: ModuleType,
    options: AuditOptions = {}
  ): Promise<AuditReport> {
    const allIssues: Issue[] = [];

    // Run all checks
    for (const check of this.checks) {
      try {
        const issues = await check.check(modulePath, moduleType);
        allIssues.push(...issues);
      } catch (error) {
        // Log check failure but continue with other checks
        console.error(
          `❌ Check "${check.name}" failed: ${
            error instanceof Error ? error.message : String(error)
          }`
        );
      }
    }

    // Apply fixes if requested
    if (options.fix) {
      await this.applyFixes(modulePath, allIssues);
    }

    // Build report
    const report: AuditReport = {
      modulePath,
      moduleType,
      timestamp: Date.now(),
      issues: allIssues,
      summary: {
        errors: allIssues.filter((i) => i.severity === 'error').length,
        warnings: allIssues.filter((i) => i.severity === 'warning').length,
        fixable: allIssues.filter((i) => i.fixable).length,
      },
    };

    return report;
  }

  /**
   * Apply fixes to fixable issues
   */
  private async applyFixes(
    modulePath: string,
    issues: Issue[]
  ): Promise<void> {
    // Group issues by category
    const issuesByCategory = new Map<string, Issue[]>();
    for (const issue of issues) {
      if (!issue.fixable) continue;

      const existing = issuesByCategory.get(issue.category) || [];
      existing.push(issue);
      issuesByCategory.set(issue.category, existing);
    }

    // Apply fixes by check
    for (const check of this.checks) {
      const checkIssues = issuesByCategory.get(check.category);
      if (!checkIssues || checkIssues.length === 0) continue;

      try {
        await check.fix(modulePath, checkIssues);
        console.log(`✅ Fixed ${checkIssues.length} ${check.category} issues`);
      } catch (error) {
        console.error(
          `❌ Failed to fix ${check.category} issues: ${
            error instanceof Error ? error.message : String(error)
          }`
        );
      }
    }
  }

  /**
   * Format audit report for CLI output
   */
  formatReport(report: AuditReport): string {
    const lines: string[] = [];

    lines.push(
      `🔍 Auditing module: ${report.modulePath} (type: ${report.moduleType})\n`
    );

    // Group issues by category
    const issuesByCategory = new Map<string, Issue[]>();
    for (const issue of report.issues) {
      const existing = issuesByCategory.get(issue.category) || [];
      existing.push(issue);
      issuesByCategory.set(issue.category, existing);
    }

    // Check results by category
    const categories = [
      'lint',
      'missing-file',
      'outdated-pattern',
      'package-json',
      'readme',
      'test-coverage',
      'hibernation-pollution',
    ];

    for (const category of categories) {
      const issues = issuesByCategory.get(category) || [];
      const errors = issues.filter((i) => i.severity === 'error');
      const warnings = issues.filter((i) => i.severity === 'warning');

      if (errors.length === 0 && warnings.length === 0) {
        lines.push(`✅ ${this.categoryLabel(category)}: No issues`);
      } else {
        const label = this.categoryLabel(category);
        if (errors.length > 0) {
          lines.push(`❌ ${label}: ${errors.length} errors`);
        }
        if (warnings.length > 0) {
          lines.push(`⚠️  ${label}: ${warnings.length} warnings`);
        }

        // Show first few issues
        const toShow = issues.slice(0, 3);
        for (const issue of toShow) {
          const icon = issue.severity === 'error' ? '  ❌' : '  ⚠️';
          const fixable = issue.fixable ? ' (fixable)' : '';
          lines.push(`${icon} ${issue.message}${fixable}`);
          if (issue.filePath) {
            const location = issue.lineNumber
              ? `${issue.filePath}:${issue.lineNumber}`
              : issue.filePath;
            lines.push(`     Location: ${location}`);
          }
        }

        if (issues.length > 3) {
          lines.push(`     ... and ${issues.length - 3} more`);
        }
      }
    }

    // Summary
    lines.push(`\n📊 Summary:`);
    lines.push(`   ${report.summary.errors} errors, ${report.summary.warnings} warnings, ${report.summary.fixable} fixable`);

    if (report.summary.fixable > 0) {
      lines.push(
        `\nRun with --fix to automatically fix issues:`
      );
      lines.push(
        `   ./jtag generate/audit --module="${report.modulePath}" --fix`
      );
    }

    return lines.join('\n');
  }

  private categoryLabel(category: string): string {
    const labels: Record<string, string> = {
      lint: 'Linting',
      'missing-file': 'Files',
      'outdated-pattern': 'Patterns',
      'package-json': 'Package.json',
      readme: 'README',
      'test-coverage': 'Tests',
      'hibernation-pollution': 'Hibernation',
    };
    return labels[category] || category;
  }
}
