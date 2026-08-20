/**
 * Package.json Validation Check
 *
 * Validates package.json has required fields and correct structure
 */

import type { IAuditCheck, Issue, ModuleType, IssueCategory } from '../AuditTypes';
import * as fs from 'fs';
import * as path from 'path';

export class PackageJsonCheck implements IAuditCheck {
  readonly name = 'Package.json Check';
  readonly category: IssueCategory = 'package-json';

  /**
   * Check package.json validity
   */
  async check(modulePath: string, moduleType: ModuleType): Promise<Issue[]> {
    const issues: Issue[] = [];
    const absolutePath = path.resolve(modulePath);
    const packageJsonPath = path.join(absolutePath, 'package.json');

    // Check if package.json exists
    if (!fs.existsSync(packageJsonPath)) {
      issues.push({
        severity: 'error',
        category: this.category,
        message: 'package.json is missing',
        filePath: 'package.json',
        fixable: true,
        suggestedFix: 'Generate package.json from template',
      });
      return issues;
    }

    // Parse package.json
    let packageJson: any;
    try {
      const content = fs.readFileSync(packageJsonPath, 'utf-8');
      packageJson = JSON.parse(content);
    } catch (error) {
      issues.push({
        severity: 'error',
        category: this.category,
        message: `package.json is invalid JSON: ${error instanceof Error ? error.message : String(error)}`,
        filePath: 'package.json',
        fixable: false,
        suggestedFix: 'Fix JSON syntax errors',
      });
      return issues;
    }

    // Validate required fields
    const requiredFields = this.getRequiredFields(moduleType);
    for (const field of requiredFields) {
      if (!packageJson[field]) {
        issues.push({
          severity: 'error',
          category: this.category,
          message: `package.json missing required field: ${field}`,
          filePath: 'package.json',
          fixable: true,
          suggestedFix: `Add ${field} field`,
        });
      }
    }

    // Validate name format (should be @jtag-{type}/{name})
    if (packageJson.name && !this.isValidNameFormat(packageJson.name, moduleType)) {
      issues.push({
        severity: 'warning',
        category: this.category,
        message: `package.json name should follow format: @jtag-${moduleType}s/{name}`,
        filePath: 'package.json',
        fixable: true,
        suggestedFix: `Rename to @jtag-${moduleType}s/{module-name}`,
      });
    }

    // Check for required scripts
    const requiredScripts = ['test', 'lint'];
    if (packageJson.scripts) {
      for (const script of requiredScripts) {
        if (!packageJson.scripts[script]) {
          issues.push({
            severity: 'warning',
            category: this.category,
            message: `package.json missing recommended script: ${script}`,
            filePath: 'package.json',
            fixable: true,
            suggestedFix: `Add ${script} script`,
          });
        }
      }
    } else {
      issues.push({
        severity: 'warning',
        category: this.category,
        message: 'package.json missing scripts section',
        filePath: 'package.json',
        fixable: true,
        suggestedFix: 'Add scripts section with test and lint',
      });
    }

    return issues;
  }

  /**
   * Fix package.json issues
   */
  async fix(modulePath: string, issues: Issue[]): Promise<void> {
    console.log(`\n⚠️  Package.json fixes require regeneration in ${modulePath}:`);
    for (const issue of issues) {
      if (issue.category === this.category) {
        console.log(`   - ${issue.message}`);
        console.log(`     Suggestion: ${issue.suggestedFix}`);
      }
    }
    console.log('\n⚠️  Run generator to regenerate package.json from template\n');
  }

  /**
   * Get required fields for module type
   */
  private getRequiredFields(moduleType: ModuleType): string[] {
    const common = ['name', 'version', 'description', 'main', 'types'];

    switch (moduleType) {
      case 'command':
        return [...common, 'scripts', 'peerDependencies'];
      case 'widget':
        return [...common, 'scripts'];
      case 'daemon':
        return [...common, 'scripts'];
      default:
        return common;
    }
  }

  /**
   * Validate name format
   */
  private isValidNameFormat(name: string, moduleType: ModuleType): boolean {
    const prefix = `@jtag-${moduleType}s/`;
    return name.startsWith(prefix);
  }
}
