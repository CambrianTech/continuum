/**
 * LintCheck - Validates linting and applies eslint --fix
 */

import { execSync } from 'child_process';
import * as path from 'path';
import type { IAuditCheck, Issue, ModuleType } from '../AuditTypes';

export class LintCheck implements IAuditCheck {
  readonly name = 'Lint Check';
  readonly category = 'lint' as const;

  async check(modulePath: string, _moduleType: ModuleType): Promise<Issue[]> {
    const issues: Issue[] = [];

    try {
      // Run eslint on the module
      const absolutePath = path.resolve(modulePath);
      execSync(`npx eslint "${absolutePath}/**/*.ts" --format json`, {
        encoding: 'utf-8',
        stdio: 'pipe',
      });

      // If we get here, no lint errors
      return [];
    } catch (error) {
      // eslint exits with non-zero if there are errors
      if (error instanceof Error && 'stdout' in error) {
        try {
          const output = (error as { stdout: string }).stdout;
          const results = JSON.parse(output);

          // Parse eslint results
          for (const result of results) {
            if (!result.messages || result.messages.length === 0) continue;

            for (const message of result.messages) {
              issues.push({
                severity: message.severity === 2 ? 'error' : 'warning',
                category: 'lint',
                message: `${message.ruleId || 'lint'}: ${message.message}`,
                filePath: result.filePath,
                lineNumber: message.line,
                fixable: message.fix !== undefined,
              });
            }
          }
        } catch (parseError) {
          // Failed to parse eslint output
          issues.push({
            severity: 'error',
            category: 'lint',
            message: 'Failed to run eslint',
            fixable: false,
          });
        }
      }
    }

    return issues;
  }

  async fix(modulePath: string, _issues: Issue[]): Promise<void> {
    try {
      const absolutePath = path.resolve(modulePath);

      // Run eslint with --fix flag
      execSync(`npx eslint "${absolutePath}/**/*.ts" --fix`, {
        encoding: 'utf-8',
        stdio: 'inherit',
      });

      console.log(`✅ Applied eslint --fix to ${modulePath}`);
    } catch (error) {
      // Some errors may remain unfixable
      const message =
        error instanceof Error ? error.message : String(error);
      console.warn(
        `⚠️  Some lint errors could not be auto-fixed: ${message}`
      );
    }
  }
}
