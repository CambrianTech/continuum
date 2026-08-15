/**
 * Test Coverage Check
 *
 * Verifies test files exist for the module
 */

import type { IAuditCheck, Issue, ModuleType, IssueCategory } from '../AuditTypes';
import * as fs from 'fs';
import * as path from 'path';

export class TestCoverageCheck implements IAuditCheck {
  readonly name = 'Test Coverage Check';
  readonly category: IssueCategory = 'test-coverage';

  /**
   * Check test coverage
   */
  async check(modulePath: string, _moduleType: ModuleType): Promise<Issue[]> {
    const issues: Issue[] = [];
    const absolutePath = path.resolve(modulePath);

    // Check for test directory
    const testDir = path.join(absolutePath, 'test');
    if (!fs.existsSync(testDir)) {
      issues.push({
        severity: 'error',
        category: this.category,
        message: 'test/ directory is missing',
        filePath: 'test/',
        fixable: true,
        suggestedFix: 'Create test/ directory with unit and integration subdirectories',
      });
      return issues;
    }

    // Check for unit tests
    const unitTestDir = path.join(testDir, 'unit');
    if (!fs.existsSync(unitTestDir)) {
      issues.push({
        severity: 'warning',
        category: this.category,
        message: 'test/unit/ directory is missing',
        filePath: 'test/unit/',
        fixable: true,
        suggestedFix: 'Create test/unit/ directory and add unit tests',
      });
    } else {
      // Check if unit test directory has test files
      const unitTests = this.findTestFiles(unitTestDir);
      if (unitTests.length === 0) {
        issues.push({
          severity: 'warning',
          category: this.category,
          message: 'No unit test files found in test/unit/',
          filePath: 'test/unit/',
          fixable: true,
          suggestedFix: 'Add *.test.ts files to test/unit/',
        });
      }
    }

    // Check for integration tests
    const integrationTestDir = path.join(testDir, 'integration');
    if (!fs.existsSync(integrationTestDir)) {
      issues.push({
        severity: 'warning',
        category: this.category,
        message: 'test/integration/ directory is missing',
        filePath: 'test/integration/',
        fixable: true,
        suggestedFix: 'Create test/integration/ directory and add integration tests',
      });
    } else {
      // Check if integration test directory has test files
      const integrationTests = this.findTestFiles(integrationTestDir);
      if (integrationTests.length === 0) {
        issues.push({
          severity: 'warning',
          category: this.category,
          message: 'No integration test files found in test/integration/',
          filePath: 'test/integration/',
          fixable: true,
          suggestedFix: 'Add *.test.ts files to test/integration/',
        });
      }
    }

    return issues;
  }

  /**
   * Fix test coverage issues
   */
  async fix(modulePath: string, issues: Issue[]): Promise<void> {
    console.log(`\n⚠️  Test coverage fixes require test file creation in ${modulePath}:`);
    for (const issue of issues) {
      if (issue.category === this.category) {
        console.log(`   - ${issue.message}`);
        console.log(`     Suggestion: ${issue.suggestedFix}`);
      }
    }
    console.log('\n⚠️  Generate test files from templates or write manually\n');
  }

  /**
   * Find test files in directory
   */
  private findTestFiles(dir: string): string[] {
    const files: string[] = [];

    if (!fs.existsSync(dir)) {
      return files;
    }

    const entries = fs.readdirSync(dir, { withFileTypes: true });

    for (const entry of entries) {
      const fullPath = path.join(dir, entry.name);

      if (entry.isFile() && entry.name.endsWith('.test.ts')) {
        files.push(fullPath);
      }
    }

    return files;
  }
}
