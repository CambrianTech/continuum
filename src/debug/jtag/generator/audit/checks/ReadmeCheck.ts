/**
 * README Completeness Check
 *
 * Validates README has required sections
 */

import type { IAuditCheck, Issue, ModuleType, IssueCategory } from '../AuditTypes';
import * as fs from 'fs';
import * as path from 'path';

export class ReadmeCheck implements IAuditCheck {
  readonly name = 'README Check';
  readonly category: IssueCategory = 'readme';

  /**
   * Check README completeness
   */
  async check(modulePath: string, moduleType: ModuleType): Promise<Issue[]> {
    const issues: Issue[] = [];
    const absolutePath = path.resolve(modulePath);
    const readmePath = path.join(absolutePath, 'README.md');

    // Check if README exists
    if (!fs.existsSync(readmePath)) {
      issues.push({
        severity: 'error',
        category: this.category,
        message: 'README.md is missing',
        filePath: 'README.md',
        fixable: true,
        suggestedFix: 'Generate README.md from template',
      });
      return issues;
    }

    // Read README content
    const content = fs.readFileSync(readmePath, 'utf-8');

    // Check for required sections
    const requiredSections = this.getRequiredSections(moduleType);
    for (const section of requiredSections) {
      if (!this.hasSect(content, section)) {
        issues.push({
          severity: 'warning',
          category: this.category,
          message: `README missing recommended section: ${section}`,
          filePath: 'README.md',
          fixable: true,
          suggestedFix: `Add ## ${section} section`,
        });
      }
    }

    // Check if README has content (not just template)
    if (content.length < 200) {
      issues.push({
        severity: 'warning',
        category: this.category,
        message: 'README appears to be incomplete (< 200 characters)',
        filePath: 'README.md',
        fixable: false,
        suggestedFix: 'Add more documentation and examples',
      });
    }

    return issues;
  }

  /**
   * Fix README issues
   */
  async fix(modulePath: string, issues: Issue[]): Promise<void> {
    console.log(`\n⚠️  README fixes require manual editing in ${modulePath}:`);
    for (const issue of issues) {
      if (issue.category === this.category) {
        console.log(`   - ${issue.message}`);
        console.log(`     Suggestion: ${issue.suggestedFix}`);
      }
    }
    console.log('\n⚠️  Add missing sections or regenerate from template\n');
  }

  /**
   * Get required sections for module type
   */
  private getRequiredSections(moduleType: ModuleType): string[] {
    const common = ['Usage', 'Parameters', 'Result', 'Examples'];

    switch (moduleType) {
      case 'command':
        return [...common, 'Testing'];
      case 'widget':
        return [...common, 'Props', 'Events'];
      case 'daemon':
        return [...common, 'Configuration'];
      default:
        return common;
    }
  }

  /**
   * Check if README has section (case-insensitive, flexible formatting)
   */
  private hasSection(content: string, section: string): boolean {
    // Match ## Section or # Section (with optional extra #'s)
    const regex = new RegExp(`^#{1,6}\\s*${section}`, 'im');
    return regex.test(content);
  }

  // Fix typo in method name
  private hasSect = this.hasSection;
}
