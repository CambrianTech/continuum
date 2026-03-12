/**
 * Code Smell Detection — Server Implementation
 *
 * Scans codebase using:
 * 1. Generator SDK audit (missing accessors, factories, specs)
 * 2. grep patterns (any casts, raw Commands.execute, Record<string, unknown>)
 * 3. File stat analysis (god classes over line threshold)
 *
 * Output is structured for sentinel consumption — each smell is a
 * discrete, verifiable task that a lesser model can fix.
 */

import * as fs from 'fs';
import * as path from 'path';
import { execSync } from 'child_process';
import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type {
  DevelopmentCodeSmellParams,
  DevelopmentCodeSmellResult,
  SmellCategory,
  CodeSmellCategoryResult,
  CodeSmellLocation
} from '../shared/DevelopmentCodeSmellTypes';
import { createDevelopmentCodeSmellResultFromParams } from '../shared/DevelopmentCodeSmellTypes';

const GOD_CLASS_THRESHOLD = 500;

export class DevelopmentCodeSmellServerCommand extends CommandBase<DevelopmentCodeSmellParams, DevelopmentCodeSmellResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/code-smell', context, subpath, commander);
  }

  async execute(params: DevelopmentCodeSmellParams): Promise<DevelopmentCodeSmellResult> {
    const srcDir = path.resolve(__dirname, '../../../..');
    const scanDir = params.path ? path.join(srcDir, params.path) : srcDir;
    const category = params.category || 'all';
    const verbose = params.verbose ?? false;

    const categories: CodeSmellCategoryResult[] = [];

    // Run requested scans
    if (category === 'all' || category === 'any-casts') {
      categories.push(this.scanAnyCasts(scanDir, srcDir, verbose));
    }
    if (category === 'all' || category === 'raw-execute') {
      categories.push(this.scanRawExecute(scanDir, srcDir, verbose));
    }
    if (category === 'all' || category === 'god-class') {
      categories.push(this.scanGodClasses(scanDir, srcDir, verbose));
    }
    if (category === 'all' || category === 'missing-accessor') {
      categories.push(this.scanMissingAccessors(srcDir, verbose));
    }
    if (category === 'all' || category === 'missing-types') {
      categories.push(this.scanMissingTypes(srcDir, verbose));
    }

    const totalSmells = categories.reduce((sum, c) => sum + c.count, 0);

    // Auto-fix if requested
    let fixed = 0;
    if (params.fix) {
      fixed = this.autoFix(srcDir);
    }

    // Build summary
    const lines: string[] = [];
    for (const cat of categories) {
      const icon = cat.count === 0 ? '✅' : cat.fixable ? '🔧' : '⚠️';
      lines.push(`${icon} ${cat.category}: ${cat.count} ${cat.description}`);
    }
    if (fixed > 0) {
      lines.push(`\n🔧 Auto-fixed ${fixed} issues`);
    }
    lines.push(`\n📊 Total: ${totalSmells} code smells`);

    return createDevelopmentCodeSmellResultFromParams(params, {
      success: true,
      totalSmells,
      categories,
      summary: lines.join('\n'),
      ...(fixed > 0 ? { fixed } : {}),
    });
  }

  // ── Scanners ─────────────────────────────────────────────────────

  private scanAnyCasts(scanDir: string, srcDir: string, verbose: boolean): CodeSmellCategoryResult {
    const locations = this.grepPattern(
      scanDir, srcDir,
      ':\\s*any\\b|as\\s+any\\b',
      '*.ts',
      ['node_modules', 'dist', 'test', '.backup']
    );
    return {
      category: 'any-casts',
      count: locations.length,
      locations: verbose ? locations : locations.slice(0, 10),
      fixable: false,
      description: 'any casts in production code',
    };
  }

  private scanRawExecute(scanDir: string, srcDir: string, verbose: boolean): CodeSmellCategoryResult {
    // Find Commands.execute('string-literal' NOT in Types files, tests, or generator
    const all = this.grepPattern(
      scanDir, srcDir,
      "Commands\\.execute\\('[a-z]",
      '*.ts',
      ['node_modules', 'dist', 'generator', 'test', '.backup', 'Types.ts']
    );
    return {
      category: 'raw-execute',
      count: all.length,
      locations: verbose ? all : all.slice(0, 10),
      fixable: true,
      description: 'raw Commands.execute() calls (should use typed accessor)',
    };
  }

  private scanGodClasses(scanDir: string, srcDir: string, verbose: boolean): CodeSmellCategoryResult {
    const locations: CodeSmellLocation[] = [];

    const walk = (dir: string) => {
      if (!fs.existsSync(dir)) return;
      for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        if (entry.name === 'node_modules' || entry.name === 'dist' || entry.name === 'test' || entry.name === '.backup') continue;
        const full = path.join(dir, entry.name);
        if (entry.isDirectory()) {
          walk(full);
        } else if (entry.name.endsWith('.ts') && !entry.name.endsWith('.d.ts') && !entry.name.endsWith('.test.ts')) {
          try {
            const content = fs.readFileSync(full, 'utf-8');
            const lineCount = content.split('\n').length;
            if (lineCount > GOD_CLASS_THRESHOLD) {
              locations.push({
                file: path.relative(srcDir, full),
                line: lineCount,
                text: `${lineCount} lines (threshold: ${GOD_CLASS_THRESHOLD})`,
              });
            }
          } catch { /* skip unreadable */ }
        }
      }
    };
    walk(scanDir);

    // Sort by line count descending
    locations.sort((a, b) => (b.line ?? 0) - (a.line ?? 0));

    return {
      category: 'god-class',
      count: locations.length,
      locations: verbose ? locations : locations.slice(0, 15),
      fixable: false,
      description: `files over ${GOD_CLASS_THRESHOLD} lines`,
    };
  }

  private scanMissingAccessors(srcDir: string, verbose: boolean): CodeSmellCategoryResult {
    try {
      const { CommandAuditor } = require(path.join(srcDir, 'generator', 'CommandAuditor'));
      const auditor = new CommandAuditor(srcDir);
      const audit = auditor.audit();
      const missing = audit.entries.filter((e: { hasStaticAccessor: boolean; hasTypesFile: boolean }) =>
        !e.hasStaticAccessor && e.hasTypesFile
      );
      const locations: CodeSmellLocation[] = missing.map((e: { commandName: string; dirPath: string }) => ({
        file: path.relative(srcDir, e.dirPath),
        text: `${e.commandName} — missing static accessor`,
      }));
      return {
        category: 'missing-accessor',
        count: locations.length,
        locations: verbose ? locations : locations.slice(0, 10),
        fixable: true,
        description: 'commands missing typed static accessor',
      };
    } catch {
      return {
        category: 'missing-accessor',
        count: -1,
        locations: [{ file: 'generator/CommandAuditor.ts', text: 'Failed to load auditor' }],
        fixable: false,
        description: 'auditor load failed',
      };
    }
  }

  private scanMissingTypes(srcDir: string, verbose: boolean): CodeSmellCategoryResult {
    try {
      const { CommandAuditor } = require(path.join(srcDir, 'generator', 'CommandAuditor'));
      const auditor = new CommandAuditor(srcDir);
      const audit = auditor.audit();
      const missing = audit.entries.filter((e: { hasTypesFile: boolean }) => !e.hasTypesFile);
      const locations: CodeSmellLocation[] = missing.map((e: { commandName: string; dirPath: string }) => ({
        file: path.relative(srcDir, e.dirPath),
        text: `${e.commandName} — no Types file`,
      }));
      return {
        category: 'missing-types',
        count: locations.length,
        locations: verbose ? locations : locations.slice(0, 10),
        fixable: true,
        description: 'commands without a Types file',
      };
    } catch {
      return {
        category: 'missing-types',
        count: -1,
        locations: [],
        fixable: false,
        description: 'auditor load failed',
      };
    }
  }

  // ── Helpers ──────────────────────────────────────────────────────

  private grepPattern(
    scanDir: string,
    srcDir: string,
    pattern: string,
    globPattern: string,
    excludes: string[]
  ): CodeSmellLocation[] {
    const regex = new RegExp(pattern);
    const ext = globPattern.replace('*', '');
    const locations: CodeSmellLocation[] = [];

    const walk = (dir: string) => {
      if (!fs.existsSync(dir)) return;
      for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        if (excludes.includes(entry.name)) continue;
        const full = path.join(dir, entry.name);
        if (entry.isDirectory()) {
          walk(full);
        } else if (entry.name.endsWith(ext)) {
          // Check if filename matches any exclude pattern (e.g. 'Types.ts')
          if (excludes.some(ex => entry.name.endsWith(ex))) continue;
          try {
            const content = fs.readFileSync(full, 'utf-8');
            const lines = content.split('\n');
            for (let i = 0; i < lines.length; i++) {
              if (regex.test(lines[i])) {
                locations.push({
                  file: path.relative(srcDir, full),
                  line: i + 1,
                  text: lines[i].trim(),
                });
              }
            }
          } catch { /* skip unreadable */ }
        }
      }
    };
    walk(scanDir);
    return locations;
  }

  private autoFix(srcDir: string): number {
    try {
      const scriptPath = path.join(srcDir, 'generator', 'add-static-accessors.ts');
      if (!fs.existsSync(scriptPath)) return 0;
      const output = execSync(`npx tsx "${scriptPath}"`, { encoding: 'utf-8', cwd: srcDir, maxBuffer: 10 * 1024 * 1024 });
      const match = output.match(/Fixed:\s+(\d+)/);
      return match ? parseInt(match[1], 10) : 0;
    } catch {
      return 0;
    }
  }
}
