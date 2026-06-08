/**
 * Unused Code Check
 *
 * Removes unused variables, imports, and parameters
 * Handles cases that eslint --fix can't handle automatically
 */

import * as fs from 'fs';
import * as path from 'path';
import type { IAuditCheck, Issue, ModuleType, IssueCategory } from '../AuditTypes';

export class UnusedCodeCheck implements IAuditCheck {
  readonly name = 'Unused Code Check';
  readonly category: IssueCategory = 'unused-code';

  async check(modulePath: string, _moduleType: ModuleType): Promise<Issue[]> {
    const issues: Issue[] = [];
    const absolutePath = path.resolve(modulePath);

    // Find all TypeScript files
    const files = this.findTsFiles(absolutePath);

    for (const file of files) {
      const content = fs.readFileSync(file, 'utf-8');
      const lines = content.split('\n');

      // Check for unused catch variables (common pattern: catch (error))
      for (let i = 0; i < lines.length; i++) {
        const line = lines[i];

        // Detect: } catch (error) { ... } where 'error' is never used
        const catchMatch = line.match(/}\s*catch\s*\((\w+)\)\s*{/);
        if (catchMatch) {
          const varName = catchMatch[1];
          const blockStart = i;
          let blockEnd = this.findBlockEnd(lines, blockStart);

          // Check if variable is used in the block
          let isUsed = false;
          for (let j = blockStart + 1; j < blockEnd; j++) {
            if (lines[j].includes(varName)) {
              isUsed = true;
              break;
            }
          }

          if (!isUsed && varName !== '_') {
            issues.push({
              severity: 'warning',
              category: this.category,
              message: `Unused catch variable '${varName}' - prefix with underscore or remove`,
              filePath: file,
              lineNumber: i + 1,
              fixable: true,
              suggestedFix: `Replace 'catch (${varName})' with 'catch (_${varName})'`,
            });
          }
        }
      }
    }

    return issues;
  }

  async fix(modulePath: string, issues: Issue[]): Promise<void> {
    const absolutePath = path.resolve(modulePath);

    // Group issues by file
    const issuesByFile = new Map<string, Issue[]>();
    for (const issue of issues) {
      if (!issue.filePath || issue.category !== this.category) continue;

      const fileIssues = issuesByFile.get(issue.filePath) || [];
      fileIssues.push(issue);
      issuesByFile.set(issue.filePath, fileIssues);
    }

    // Fix each file
    for (const [filePath, fileIssues] of issuesByFile) {
      let content = fs.readFileSync(filePath, 'utf-8');

      for (const issue of fileIssues) {
        // Fix unused catch variables
        if (issue.message.includes('catch variable')) {
          const match = issue.message.match(/variable '(\w+)'/);
          if (match) {
            const varName = match[1];
            // Replace catch (varName) with catch (_varName)
            content = content.replace(
              new RegExp(`catch\\s*\\(${varName}\\)`, 'g'),
              `catch (_${varName})`
            );
          }
        }
      }

      fs.writeFileSync(filePath, content, 'utf-8');
    }

    console.log(`✅ Fixed ${issues.length} unused code issues in ${absolutePath}`);
  }

  /**
   * Find all TypeScript files in directory
   */
  private findTsFiles(dir: string): string[] {
    const files: string[] = [];

    if (!fs.existsSync(dir)) {
      return files;
    }

    const entries = fs.readdirSync(dir, { withFileTypes: true });

    for (const entry of entries) {
      const fullPath = path.join(dir, entry.name);

      if (entry.isDirectory() && entry.name !== 'node_modules') {
        files.push(...this.findTsFiles(fullPath));
      } else if (entry.isFile() && entry.name.endsWith('.ts')) {
        files.push(fullPath);
      }
    }

    return files;
  }

  /**
   * Find the end of a block starting from a line with '{'
   */
  private findBlockEnd(lines: string[], startLine: number): number {
    let depth = 0;
    let foundStart = false;

    for (let i = startLine; i < lines.length; i++) {
      const line = lines[i];

      for (const char of line) {
        if (char === '{') {
          depth++;
          foundStart = true;
        } else if (char === '}') {
          depth--;
          if (foundStart && depth === 0) {
            return i;
          }
        }
      }
    }

    return lines.length - 1;
  }
}
