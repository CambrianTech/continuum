/**
 * Command Constants Generator
 *
 * Automatically generates COMMANDS constant object from command directory structure.
 * This provides a single source of truth for command names and eliminates hardcoded strings.
 *
 * How it works:
 * 1. Glob for all command Type files (commands slash star star slash shared slash star Types.ts)
 * 2. Derive command names from file paths (same logic as generate-command-schemas.ts)
 * 3. Generate constant names (workspace/task/list to WORKSPACE_TASK_LIST)
 * 4. Write to shared/generated-command-constants.ts
 *
 * **Integration:**
 * - Runs automatically via prebuild script in package.json
 * - Import { COMMANDS } from '@shared/generated-command-constants'
 * - Never hardcode command strings (except 'list' for bootstrap)
 *
 * **Why this matters:**
 * - Commands can be renamed/moved without breaking references
 * - Type-safe command name usage throughout codebase
 * - Auto-discovery - add command, run build, constant appears
 * - Single source of truth (file system structure)
 */

import { readFileSync, writeFileSync } from 'fs';
import { join, relative } from 'path';
import * as glob from 'glob';

// ============================================================================
// COMMAND DISCOVERY
// ============================================================================

class CommandConstantsGenerator {
  private rootPath: string;
  private commands: Set<string> = new Set();

  constructor(rootPath: string) {
    this.rootPath = rootPath;
  }

  /**
   * Main entry point - discover all commands and generate constants
   */
  generate(): void {
    console.log('🔍 Discovering commands from file structure...');

    // Find all *Types.ts files in commands/**/shared/
    const pattern = join(this.rootPath, 'commands/**/shared/*Types.ts');
    const files = glob.sync(pattern);

    console.log(`📄 Found ${files.length} Type files`);

    for (const filePath of files) {
      try {
        const commandNames = this.extractCommandNamesFromFile(filePath);
        for (const name of commandNames) {
          this.commands.add(name);
        }
      } catch (error) {
        console.error(`  ❌ Failed to extract from ${filePath}:`, error);
      }
    }

    // Sort commands for consistent output
    const sortedCommands = Array.from(this.commands).sort();

    console.log(`\n✅ Discovered ${sortedCommands.length} commands`);

    this.writeConstants(sortedCommands);
  }

  /**
   * Extract command names from a single *Types.ts file
   * Uses same logic as generate-command-schemas.ts
   */
  private extractCommandNamesFromFile(filePath: string): string[] {
    const content = readFileSync(filePath, 'utf-8');
    const relativePath = relative(this.rootPath, filePath);

    // Extract base command path from file path: commands/wall/shared/WallTypes.ts → wall
    const commandPathMatch = relativePath.match(/commands\/(.+?)\/shared\/\w+Types\.ts$/);
    if (!commandPathMatch) {
      return [];
    }

    const basePath = commandPathMatch[1];

    // Find ALL *Params interfaces that extend CommandParams
    const paramsInterfaceRegex = /export\s+interface\s+(\w+Params)\s+extends\s+(\w+)\s*\{/g;
    const commandNames: string[] = [];
    let match;

    while ((match = paramsInterfaceRegex.exec(content)) !== null) {
      const interfaceName = match[1];
      const commandName = this.deriveCommandName(interfaceName, basePath);
      commandNames.push(commandName);
    }

    return commandNames;
  }

  /**
   * Derive command name from interface name and base path
   * Same logic as generate-command-schemas.ts
   */
  private deriveCommandName(interfaceName: string, basePath: string): string {
    // Just return basePath directly - it already contains the full command path
    // from the file structure (e.g., "workspace/task/list")
    return basePath;
  }

  /**
   * Convert command path to constant name
   * workspace/task/list → WORKSPACE_TASK_LIST
   * ai/generate → AI_GENERATE
   */
  private toConstantName(commandPath: string): string {
    return commandPath
      .toUpperCase()
      .replace(/\//g, '_')
      .replace(/-/g, '_');
  }

  /**
   * Write generated constants to shared/generated-command-constants.ts
   */
  private writeConstants(commands: string[]): void {
    const outputPath = join(this.rootPath, 'shared/generated-command-constants.ts');

    const lines: string[] = [];

    // File header
    lines.push('/**');
    lines.push(' * Command Names - SINGLE SOURCE OF TRUTH');
    lines.push(' * ');
    lines.push(' * ⚠️ AUTO-GENERATED - DO NOT EDIT MANUALLY');
    lines.push(' * ⚠️ Generated from command directory structure via generator/generate-command-constants.ts');
    lines.push(' * ⚠️ Regenerate: npm run prebuild (or npm run build:ts)');
    lines.push(' * ');
    lines.push(' * **Usage:**');
    lines.push(' * ```typescript');
    lines.push(' * import { COMMANDS } from \'@shared/generated-command-constants\';');
    lines.push(' * ');
    lines.push(' * // Good:');
    lines.push(' * await Commands.execute(COMMANDS.DATA_LIST, params);');
    lines.push(' * if (command === COMMANDS.SESSION_CREATE) { ... }');
    lines.push(' * ');
    lines.push(' * // Bad (NEVER do this):');
    lines.push(' * await Commands.execute(\'data/list\', params); // ❌ Hardcoded string');
    lines.push(' * if (command === \'session/create\') { ... }  // ❌ Magic string');
    lines.push(' * ```');
    lines.push(' * ');
    lines.push(' * **Exception:**');
    lines.push(' * Only \'list\' command may be hardcoded (bootstrap requirement before constants load)');
    lines.push(' */');
    lines.push('');

    // Constants object
    lines.push('export const COMMANDS = {');
    for (const cmd of commands) {
      const constName = this.toConstantName(cmd);
      lines.push(`  ${constName}: '${cmd}',`);
    }
    lines.push('} as const;');
    lines.push('');

    // Type-safe command name type
    lines.push('/** Type-safe command name type for autocomplete and type checking */');
    lines.push('export type CommandName = typeof COMMANDS[keyof typeof COMMANDS];');
    lines.push('');

    writeFileSync(outputPath, lines.join('\n'));
    console.log(`\n📝 Generated: ${relative(this.rootPath, outputPath)}`);
    console.log(`   ${commands.length} command constants`);
  }
}

// ============================================================================
// MAIN
// ============================================================================

const generator = new CommandConstantsGenerator(process.cwd());
generator.generate();
