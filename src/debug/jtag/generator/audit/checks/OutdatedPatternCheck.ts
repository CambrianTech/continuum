/**
 * Outdated Pattern Check
 *
 * Detects deprecated patterns and suggests modern alternatives
 */

import type { IAuditCheck, Issue, ModuleType, IssueCategory } from '../AuditTypes';
import * as fs from 'fs';
import * as path from 'path';

interface PatternRule {
  pattern: RegExp;
  message: string;
  suggestedFix: string;
  severity: 'error' | 'warning';
}

export class OutdatedPatternCheck implements IAuditCheck {
  readonly name = 'Outdated Pattern Check';
  readonly category: IssueCategory = 'outdated-pattern';

  private readonly patterns: PatternRule[] = [
    {
      pattern: /:\s*any\b/g,
      message: 'Use of `any` type detected - should use proper typing',
      suggestedFix: 'Replace `any` with specific type or generic',
      severity: 'warning',
    },
    {
      pattern: /interface\s+\w+\s+extends\s+\w+\s*\{\s*\}/g,
      message: 'Empty interface extending base type - unnecessary',
      suggestedFix: 'Remove empty interface or add properties',
      severity: 'warning',
    },
    {
      pattern: /console\.log\(/g,
      message: 'console.log detected - should use logger',
      suggestedFix: 'Use proper logging system instead of console.log',
      severity: 'warning',
    },
    {
      pattern: /\/\/ TODO:/g,
      message: 'TODO comment detected',
      suggestedFix: 'Resolve TODO or create issue',
      severity: 'warning',
    },
    {
      pattern: /throw new Error\(['"]Not implemented['"]\)/g,
      message: 'Unimplemented function detected',
      suggestedFix: 'Implement function or remove stub',
      severity: 'error',
    },
  ];

  /**
   * Check for outdated patterns
   */
  async check(modulePath: string, _moduleType: ModuleType): Promise<Issue[]> {
    const issues: Issue[] = [];
    const absolutePath = path.resolve(modulePath);

    // Find all TypeScript files
    const tsFiles = this.findTsFiles(absolutePath);

    for (const file of tsFiles) {
      const relativePath = path.relative(absolutePath, file);
      const content = fs.readFileSync(file, 'utf-8');
      const lines = content.split('\n');

      // Check each pattern
      for (const rule of this.patterns) {
        const matches = content.matchAll(rule.pattern);
        for (const match of matches) {
          // Find line number
          const lineNumber = this.findLineNumber(content, match.index || 0);

          issues.push({
            severity: rule.severity,
            category: this.category,
            message: rule.message,
            filePath: relativePath,
            lineNumber,
            fixable: false, // Manual review required
            suggestedFix: rule.suggestedFix,
          });
        }
      }
    }

    return issues;
  }

  /**
   * Fix outdated patterns (manual review required)
   */
  async fix(modulePath: string, issues: Issue[]): Promise<void> {
    console.log(`\n⚠️  Outdated patterns require manual review in ${modulePath}:`);

    // Group issues by file
    const byFile = new Map<string, Issue[]>();
    for (const issue of issues) {
      if (issue.category === this.category && issue.filePath) {
        const existing = byFile.get(issue.filePath) || [];
        existing.push(issue);
        byFile.set(issue.filePath, existing);
      }
    }

    // Display grouped issues
    for (const [file, fileIssues] of byFile) {
      console.log(`\n   📄 ${file}:`);
      for (const issue of fileIssues) {
        console.log(`      Line ${issue.lineNumber}: ${issue.message}`);
        console.log(`      Suggestion: ${issue.suggestedFix}`);
      }
    }

    console.log('\n⚠️  These patterns require manual code review and refactoring\n');
  }

  /**
   * Recursively find all .ts files in directory
   */
  private findTsFiles(dir: string): string[] {
    const files: string[] = [];

    if (!fs.existsSync(dir)) {
      return files;
    }

    const entries = fs.readdirSync(dir, { withFileTypes: true });

    for (const entry of entries) {
      const fullPath = path.join(dir, entry.name);

      if (entry.isDirectory()) {
        // Skip node_modules and hidden directories
        if (entry.name !== 'node_modules' && !entry.name.startsWith('.')) {
          files.push(...this.findTsFiles(fullPath));
        }
      } else if (entry.name.endsWith('.ts') && !entry.name.endsWith('.d.ts')) {
        files.push(fullPath);
      }
    }

    return files;
  }

  /**
   * Find line number from character index
   */
  private findLineNumber(content: string, index: number): number {
    const lines = content.substring(0, index).split('\n');
    return lines.length;
  }
}
