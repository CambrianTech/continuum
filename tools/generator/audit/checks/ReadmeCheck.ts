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
    const absolutePath = path.resolve(modulePath);
    const readmePath = path.join(absolutePath, 'README.md');

    // Check if README is completely missing
    const isMissing = issues.some((i) =>
      i.message.includes('README.md is missing')
    );

    if (isMissing) {
      // Generate complete README from scratch
      const { ReadmeGenerator } = await import('../utils/ReadmeGenerator');
      const content = ReadmeGenerator.generate(modulePath, 'command');
      fs.writeFileSync(readmePath, content, 'utf-8');
      console.log(`✅ Generated README.md for ${modulePath}`);
    } else {
      // README exists but has missing sections - append them
      let content = fs.readFileSync(readmePath, 'utf-8');
      const missingSections = issues
        .filter((i) => i.message.includes('missing recommended section'))
        .map((i) => {
          const match = i.message.match(/section: (\w+)/);
          return match ? match[1] : null;
        })
        .filter((s): s is string => s !== null);

      for (const section of missingSections) {
        if (!content.includes(`## ${section}`)) {
          content += `\n\n## ${section}\n\nTODO: Add ${section.toLowerCase()} documentation\n`;
        }
      }

      fs.writeFileSync(readmePath, content, 'utf-8');
      console.log(`✅ Added missing sections to README.md in ${modulePath}`);
    }
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
