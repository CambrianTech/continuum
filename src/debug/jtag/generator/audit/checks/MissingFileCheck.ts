/**
 * Missing File Check
 *
 * Detects when required files are missing from a module
 */

import type { IAuditCheck, Issue, ModuleType, IssueCategory } from '../AuditTypes';
import * as fs from 'fs';
import * as path from 'path';

export class MissingFileCheck implements IAuditCheck {
  readonly name = 'Missing File Check';
  readonly category: IssueCategory = 'missing-file';

  /**
   * Check for missing required files
   */
  async check(modulePath: string, moduleType: ModuleType): Promise<Issue[]> {
    const issues: Issue[] = [];
    const absolutePath = path.resolve(modulePath);

    // Define required directories/patterns based on module type
    const requiredStructure = this.getRequiredStructure(moduleType);

    for (const requirement of requiredStructure) {
      if (requirement.type === 'directory') {
        // Check if directory exists and has at least one .ts file
        const dirPath = path.join(absolutePath, requirement.path);
        if (!fs.existsSync(dirPath)) {
          issues.push({
            severity: 'error',
            category: this.category,
            message: `Missing required directory: ${requirement.path}`,
            filePath: requirement.path,
            fixable: true,
            suggestedFix: `Create ${requirement.path} directory with appropriate files`,
          });
        } else {
          // Check if directory has at least one .ts file
          const files = fs.readdirSync(dirPath);
          const hasTs = files.some((f) => f.endsWith('.ts'));
          if (!hasTs) {
            issues.push({
              severity: 'error',
              category: this.category,
              message: `Directory ${requirement.path} exists but has no TypeScript files`,
              filePath: requirement.path,
              fixable: true,
              suggestedFix: `Generate TypeScript files in ${requirement.path}`,
            });
          }
        }
      } else if (requirement.type === 'file') {
        // Check for specific file
        const filePath = path.join(absolutePath, requirement.path);
        if (!fs.existsSync(filePath)) {
          issues.push({
            severity: 'error',
            category: this.category,
            message: `Missing required file: ${requirement.path}`,
            filePath: requirement.path,
            fixable: true,
            suggestedFix: `Generate ${requirement.path} from template`,
          });
        }
      }
    }

    return issues;
  }

  /**
   * Fix missing files by generating them from templates
   */
  async fix(modulePath: string, issues: Issue[]): Promise<void> {
    // TODO: Implement file generation from templates
    // For now, just log what would be generated
    console.log(`\n📝 Would generate missing files in ${modulePath}:`);
    for (const issue of issues) {
      if (issue.category === this.category && issue.filePath) {
        console.log(`   - ${issue.filePath}`);
      }
    }
    console.log('\n⚠️  File generation not yet implemented (Phase 2 TODO)\n');
  }

  /**
   * Get required structure for a module type
   */
  private getRequiredStructure(
    moduleType: ModuleType
  ): Array<{ type: 'file' | 'directory'; path: string }> {
    switch (moduleType) {
      case 'command':
        return [
          { type: 'directory', path: 'shared' }, // Should have *Types.ts
          { type: 'directory', path: 'server' }, // Should have *ServerCommand.ts
          { type: 'directory', path: 'browser' }, // Should have *BrowserCommand.ts
          { type: 'file', path: 'README.md' },
          { type: 'file', path: 'package.json' },
          { type: 'file', path: '.npmignore' },
          { type: 'directory', path: 'test/unit' }, // Should have *.test.ts
          { type: 'directory', path: 'test/integration' }, // Should have *.test.ts
        ];
      case 'widget':
        return [
          // TODO: Define widget requirements
          { type: 'directory', path: 'shared' },
          { type: 'directory', path: 'browser' },
          { type: 'file', path: 'README.md' },
          { type: 'file', path: 'package.json' },
        ];
      case 'daemon':
        return [
          // TODO: Define daemon requirements
          { type: 'directory', path: 'shared' },
          { type: 'directory', path: 'server' },
          { type: 'file', path: 'README.md' },
          { type: 'file', path: 'package.json' },
        ];
      default:
        return [];
    }
  }
}
