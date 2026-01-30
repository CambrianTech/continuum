/**
 * Command Executor Generator
 *
 * Discovers all command Types files and ensures each has a type-safe executor export.
 * Idempotent — safe to run repeatedly, only modifies files that need updating.
 *
 * What it does:
 * 1. Glob for all command shared Types.ts files
 * 2. For each file, find Params interfaces that extend CommandParams (or derived)
 * 3. Check if corresponding executor already exists
 * 4. If missing, add imports + executor export at bottom of file
 *
 * Run: npx tsx generator/generate-command-executors.ts
 *      npx tsx generator/generate-command-executors.ts --dry-run
 */

import { readFileSync, writeFileSync } from 'fs';
import { join, relative } from 'path';
import * as glob from 'glob';

// ============================================================================
// TYPES
// ============================================================================

interface ExecutorInfo {
  /** PascalCase class name (e.g., "DataList", "Ping") */
  className: string;
  /** Command path (e.g., "data/list", "ping") */
  commandPath: string;
  /** Name of the parent interface (e.g., "CommandParams", "BaseDataParams") */
  parentInterface: string;
  /** Whether the Result type has a generic parameter <T extends BaseEntity> */
  resultIsGeneric: boolean;
  /** Name of the Result interface */
  resultInterfaceName: string;
}

interface FileReport {
  filePath: string;
  relativePath: string;
  executorsFound: string[];
  executorsAdded: string[];
  skipped: string[];
  error?: string;
}

// ============================================================================
// EXECUTOR GENERATOR
// ============================================================================

class CommandExecutorGenerator {
  private rootPath: string;
  private dryRun: boolean;
  private reports: FileReport[] = [];

  constructor(rootPath: string, dryRun: boolean) {
    this.rootPath = rootPath;
    this.dryRun = dryRun;
  }

  generate(): FileReport[] {
    console.log('🔧 Command Executor Generator\n');
    if (this.dryRun) {
      console.log('   ⚡ DRY RUN — no files will be modified\n');
    }

    // Find all *Types.ts files in commands/**/shared/
    const pattern = join(this.rootPath, 'commands/**/shared/*Types.ts');
    const files = glob.sync(pattern);

    console.log(`📄 Found ${files.length} Type files\n`);

    for (const filePath of files) {
      this.processFile(filePath);
    }

    return this.reports;
  }

  private processFile(filePath: string): void {
    const relativePath = relative(this.rootPath, filePath);
    const report: FileReport = {
      filePath,
      relativePath,
      executorsFound: [],
      executorsAdded: [],
      skipped: [],
    };

    try {
      const content = readFileSync(filePath, 'utf-8');

      // Skip base/shared type files that aren't actual commands
      if (this.isBaseTypeFile(relativePath)) {
        report.skipped.push('Base type file (not a command)');
        this.reports.push(report);
        return;
      }

      // Find all *Params interfaces
      const executors = this.discoverExecutors(content, relativePath);

      if (executors.length === 0) {
        report.skipped.push('No Params interfaces found');
        this.reports.push(report);
        return;
      }

      let modified = content;
      let needsCommandInputImport = false;
      let needsDataCommandInputImport = false;
      let needsCommandsImport = false;

      for (const executor of executors) {
        // Determine which input type to use
        const usesBaseDataParams = this.extendsBaseDataParams(executor.parentInterface, content);

        // Check if executor already exists
        const executorPattern = new RegExp(
          `export\\s+const\\s+${executor.className}\\s*=\\s*\\{`
        );

        if (executorPattern.test(content)) {
          report.executorsFound.push(executor.className);
          // Still ensure imports are present for existing executors
          if (usesBaseDataParams) {
            needsDataCommandInputImport = true;
          } else {
            needsCommandInputImport = true;
          }
          needsCommandsImport = true;
          continue;
        }

        if (usesBaseDataParams) {
          needsDataCommandInputImport = true;
        } else {
          needsCommandInputImport = true;
        }
        needsCommandsImport = true;

        // Generate executor code
        const executorCode = this.generateExecutor(executor, usesBaseDataParams);
        modified += '\n' + executorCode;
        report.executorsAdded.push(executor.className);
      }

      // Add missing imports (also fixes imports for already-existing executors)
      if (needsCommandInputImport || needsDataCommandInputImport || needsCommandsImport) {
        modified = this.addMissingImports(modified, {
          commandInput: needsCommandInputImport,
          dataCommandInput: needsDataCommandInputImport,
          commands: needsCommandsImport,
        }, relativePath);
      }

      // Write if modified
      if (modified !== content) {
        if (!this.dryRun) {
          writeFileSync(filePath, modified, 'utf-8');
        }
      }
    } catch (error) {
      report.error = error instanceof Error ? error.message : String(error);
    }

    this.reports.push(report);
  }

  /**
   * Skip base type files that aren't actual commands
   */
  private isBaseTypeFile(relativePath: string): boolean {
    const skipPatterns = [
      'commands/data/shared/BaseDataTypes.ts',
      'commands/data/shared/DataCommandConstants.ts',
      'commands/file/shared/FileTypes.ts',
      'commands/genome/shared/GenomeTypes.ts',
      'commands/theme/shared/ThemeTypes.ts',
    ];
    return skipPatterns.some(p => relativePath.endsWith(p));
  }

  /**
   * Discover all Params interfaces and build executor info
   */
  private discoverExecutors(content: string, relativePath: string): ExecutorInfo[] {
    const executors: ExecutorInfo[] = [];

    // Find all *Params interfaces extending something
    const paramsRegex = /export\s+interface\s+(\w+Params)\s+extends\s+(\w+)\s*\{/g;
    let match;
    const allInterfaceNames: string[] = [];
    const matches: Array<{ name: string; parent: string }> = [];

    while ((match = paramsRegex.exec(content)) !== null) {
      allInterfaceNames.push(match[1]);
      matches.push({ name: match[1], parent: match[2] });
    }

    // Derive command base path from file path
    const pathMatch = relativePath.match(/commands\/(.+?)\/shared\/\w+Types\.ts$/);
    if (!pathMatch) return executors;
    const basePath = pathMatch[1];

    for (const { name: interfaceName, parent: parentInterface } of matches) {
      // Validate it's actually a command params (extends CommandParams chain)
      if (!this.isCommandParamsDescendant(parentInterface, content)) {
        continue;
      }

      // Derive class name: remove "Params" suffix
      const className = interfaceName.replace(/Params$/, '');

      // Derive command path (handle multi-interface files)
      const commandPath = this.deriveCommandPath(className, basePath, allInterfaceNames);

      // Check for generic Result type
      const resultInterfaceName = className + 'Result';
      const resultIsGeneric = this.isResultGeneric(resultInterfaceName, content);

      executors.push({
        className,
        commandPath,
        parentInterface,
        resultIsGeneric,
        resultInterfaceName,
      });
    }

    return executors;
  }

  /**
   * Derive the command path from class name and base path
   * Same logic as generate-command-schemas.ts
   */
  private deriveCommandPath(className: string, basePath: string, allInterfaceNames: string[]): string {
    const kebabCase = className
      .replace(/([A-Z]+)([A-Z][a-z])/g, '$1-$2')
      .replace(/([a-z])([A-Z])/g, '$1-$2')
      .toLowerCase();

    const basePathSegments = basePath.split('/');
    const lastSegment = basePathSegments[basePathSegments.length - 1];

    // Multi-interface subcommand detection
    if (allInterfaceNames.length > 1 && kebabCase.startsWith(lastSegment + '-')) {
      const subcommand = kebabCase.substring(lastSegment.length + 1);
      return `${basePath}/${subcommand}`;
    }

    return basePath;
  }

  /**
   * Check if an interface ultimately descends from CommandParams
   */
  private isCommandParamsDescendant(interfaceName: string, content: string, visited = new Set<string>()): boolean {
    if (visited.has(interfaceName)) return false;
    visited.add(interfaceName);

    // Known command param base types
    const knownBases = ['CommandParams', 'BaseDataParams', 'JTAGPayload'];
    if (knownBases.includes(interfaceName)) return true;

    // Look for the interface definition in this file
    const regex = new RegExp(`export\\s+interface\\s+${interfaceName}\\s+extends\\s+(\\w+)`);
    const match = content.match(regex);
    if (match) {
      return this.isCommandParamsDescendant(match[1], content, visited);
    }

    return false;
  }

  /**
   * Check if a Result interface has generic type parameters
   */
  private isResultGeneric(resultName: string, content: string): boolean {
    const regex = new RegExp(`export\\s+interface\\s+${resultName}<`);
    return regex.test(content);
  }

  /**
   * Check if an interface extends BaseDataParams (directly or indirectly)
   */
  private extendsBaseDataParams(interfaceName: string, content: string, visited = new Set<string>()): boolean {
    if (visited.has(interfaceName)) return false;
    visited.add(interfaceName);

    if (interfaceName === 'BaseDataParams') return true;
    if (interfaceName === 'CommandParams') return false;

    const regex = new RegExp(`export\\s+interface\\s+${interfaceName}\\s+extends\\s+(\\w+)`);
    const match = content.match(regex);
    if (match) {
      return this.extendsBaseDataParams(match[1], content, visited);
    }

    return false;
  }

  /**
   * Generate executor code for a command
   */
  private generateExecutor(info: ExecutorInfo, usesBaseDataParams: boolean): string {
    const inputType = usesBaseDataParams
      ? `DataCommandInput<${info.className}Params>`
      : `CommandInput<${info.className}Params>`;

    // All params optional? (e.g., Ping, ChatExport)
    const hasRequiredParams = this.hasRequiredParams(info);

    const paramsOptional = !hasRequiredParams;
    const paramsSig = paramsOptional
      ? `params?: ${inputType}`
      : `params: ${inputType}`;

    if (info.resultIsGeneric) {
      // Generic executor with <T extends BaseEntity>
      return `/**
 * ${info.className} — Type-safe command executor
 *
 * Usage:
 *   import { ${info.className} } from '...shared/${info.className}Types';
 *   const result = await ${info.className}.execute({ ... });
 */
export const ${info.className} = {
  execute<T extends BaseEntity = BaseEntity>(${paramsSig}): Promise<${info.resultInterfaceName}<T>> {
    return Commands.execute<${info.className}Params, ${info.resultInterfaceName}<T>>('${info.commandPath}', params as Partial<${info.className}Params>);
  },
  commandName: '${info.commandPath}' as const,
} as const;
`;
    } else {
      return `/**
 * ${info.className} — Type-safe command executor
 *
 * Usage:
 *   import { ${info.className} } from '...shared/${info.className}Types';
 *   const result = await ${info.className}.execute({ ... });
 */
export const ${info.className} = {
  execute(${paramsSig}): Promise<${info.resultInterfaceName}> {
    return Commands.execute<${info.className}Params, ${info.resultInterfaceName}>('${info.commandPath}', params as Partial<${info.className}Params>);
  },
  commandName: '${info.commandPath}' as const,
} as const;
`;
    }
  }

  /**
   * Rough heuristic: check if the command likely has no required params
   * (all fields in Params are optional or inherited from CommandParams)
   */
  private hasRequiredParams(info: ExecutorInfo): boolean {
    // Commands where all user-facing params are optional
    // This is a conservative heuristic — defaults to "has required params"
    // The build will catch any mistakes
    return true;
  }

  /**
   * Add missing imports to file content
   */
  private addMissingImports(
    content: string,
    needs: { commandInput: boolean; dataCommandInput: boolean; commands: boolean },
    relativePath: string
  ): string {
    let result = content;

    // Check imports section only (not executor code which also contains these names)
    const hasCommandInputImport = /import\s+type\s+\{[^}]*\bCommandInput\b[^}]*\}/.test(result);
    const hasDataCommandInputImport = /import\s+type\s+\{[^}]*\bDataCommandInput\b[^}]*\}/.test(result);
    const hasCommandsImport = /import\s+\{[^}]*\bCommands\b[^}]*\}\s+from/.test(result);

    // Add CommandInput to existing JTAGTypes import
    if (needs.commandInput && !hasCommandInputImport) {
      // Try to add to existing JTAGTypes type import
      const jtagTypeImport = /import\s+type\s+\{([^}]+)\}\s+from\s+['"]([^'"]*JTAGTypes)['"]/;
      const match = result.match(jtagTypeImport);
      if (match) {
        const existingImports = match[1];
        if (!existingImports.includes('CommandInput')) {
          const newImports = existingImports.trimEnd() + ', CommandInput';
          result = result.replace(match[0], `import type {${newImports}} from '${match[2]}'`);
        }
      } else {
        // No existing JTAGTypes type import — add one
        const depthToSystem = this.getRelativeDepthToSystem(relativePath);
        result = `import type { CommandInput } from '${depthToSystem}system/core/types/JTAGTypes';\n` + result;
      }
    }

    // Add DataCommandInput to existing BaseDataTypes import
    if (needs.dataCommandInput && !hasDataCommandInputImport) {
      const baseDataImport = /import\s+type\s+\{([^}]+)\}\s+from\s+['"]([^'"]*BaseDataTypes)['"]/;
      const match = result.match(baseDataImport);
      if (match) {
        const existingImports = match[1];
        if (!existingImports.includes('DataCommandInput')) {
          const newImports = existingImports.trimEnd() + ', DataCommandInput';
          result = result.replace(match[0], `import type {${newImports}} from '${match[2]}'`);
        }
      } else {
        // No existing BaseDataTypes import — add one
        const depthToData = this.getRelativeDepthToDataShared(relativePath);
        if (depthToData) {
          result = `import type { DataCommandInput } from '${depthToData}commands/data/shared/BaseDataTypes';\n` + result;
        }
      }
    }

    // Add Commands import
    if (needs.commands && !hasCommandsImport) {
      const depthToSystem = this.getRelativeDepthToSystem(relativePath);
      // Find the end of the last complete import statement (handles multi-line imports)
      // Look for the last `from '...';\n` or `from "...";\n` pattern
      const lastFromMatch = [...result.matchAll(/from\s+['"][^'"]+['"];\s*\n/g)];
      if (lastFromMatch.length > 0) {
        const lastMatch = lastFromMatch[lastFromMatch.length - 1];
        const insertPos = lastMatch.index! + lastMatch[0].length;
        const importLine = `import { Commands } from '${depthToSystem}system/core/shared/Commands';\n`;
        result = result.substring(0, insertPos) + importLine + result.substring(insertPos);
      } else {
        result = `import { Commands } from '${this.getRelativeDepthToSystem(relativePath)}system/core/shared/Commands';\n` + result;
      }
    }

    return result;
  }

  /**
   * Calculate relative path prefix from a Types file to the system/ directory
   * e.g., commands/data/list/shared/DataListTypes.ts → ../../../../
   *        commands/ping/shared/PingTypes.ts → ../../../
   */
  private getRelativeDepthToSystem(relativePath: string): string {
    // Count directory depth from commands/ root
    const parts = relativePath.split('/');
    // commands/X/shared/File.ts → 4 parts → ../../../ (3 levels up)
    // commands/X/Y/shared/File.ts → 5 parts → ../../../../ (4 levels up)
    const depth = parts.length - 1; // -1 for the filename
    return '../'.repeat(depth);
  }

  /**
   * Calculate relative path to commands/data/shared/
   */
  private getRelativeDepthToDataShared(relativePath: string): string | null {
    // Only relevant for files inside commands/
    if (!relativePath.startsWith('commands/')) return null;
    const parts = relativePath.split('/');
    const depth = parts.length - 1;
    return '../'.repeat(depth) + 'commands/data/shared/';
  }
}

// ============================================================================
// MAIN
// ============================================================================

function main() {
  const rootPath = process.cwd();
  const dryRun = process.argv.includes('--dry-run');

  const generator = new CommandExecutorGenerator(rootPath, dryRun);
  const reports = generator.generate();

  // Summary
  let totalAdded = 0;
  let totalExisting = 0;
  let totalSkipped = 0;
  let totalErrors = 0;

  for (const report of reports) {
    if (report.error) {
      console.error(`  ❌ ${report.relativePath}: ${report.error}`);
      totalErrors++;
    } else if (report.executorsAdded.length > 0) {
      console.log(`  ✅ ${report.relativePath}: +${report.executorsAdded.join(', +')}`);
      totalAdded += report.executorsAdded.length;
    } else if (report.executorsFound.length > 0) {
      totalExisting += report.executorsFound.length;
    } else if (report.skipped.length > 0) {
      totalSkipped++;
    }
  }

  console.log(`\n📊 Summary:`);
  console.log(`   Added:    ${totalAdded} executors`);
  console.log(`   Existing: ${totalExisting} (already had executors)`);
  console.log(`   Skipped:  ${totalSkipped} files (base types, no params)`);
  if (totalErrors > 0) {
    console.log(`   Errors:   ${totalErrors}`);
  }
  console.log(dryRun ? '\n   ⚡ DRY RUN — no files modified' : '\n   ✨ Done!');
}

if (require.main === module) {
  try {
    main();
  } catch (error) {
    console.error('❌ Executor generation failed:', error);
    process.exit(1);
  }
}
