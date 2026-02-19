/**
 * Migrate Commands.execute Calls to Static Executors
 *
 * Finds all Commands.execute<P, R>(COMMAND_REF, params) calls
 * and replaces them with the type-safe static executor pattern:
 *   DataList.execute<T>(params)
 *
 * Handles:
 * - DATA_COMMANDS.LIST, FILE_COMMANDS.LOAD, STATE_COMMANDS.GET, etc.
 * - Hardcoded string literals: 'data/list', 'collaboration/chat/send'
 * - Generic type extraction: <DataListParams, DataListResult<UserEntity>> → .execute<UserEntity>()
 * - Import management: adds executor import, preserves existing imports
 *
 * Run: npx tsx scripts/migrate-to-static-executors.ts
 *      npx tsx scripts/migrate-to-static-executors.ts --dry-run
 */

import { readFileSync, writeFileSync } from 'fs';
import { join, relative, dirname } from 'path';
import * as glob from 'glob';

// ============================================================================
// TYPES
// ============================================================================

interface ExecutorMapping {
  /** Executor name (e.g., "DataList", "Ping") */
  executorName: string;
  /** Import path relative to rootPath, no extension (e.g., "commands/data/list/shared/DataListTypes") */
  importModule: string;
  /** Whether the execute() method has a generic <T> parameter */
  hasGenericResult: boolean;
}

interface FileReport {
  filePath: string;
  replacements: number;
  executorsUsed: string[];
  errors: string[];
}

// ============================================================================
// BUILD MAPS
// ============================================================================

/**
 * Parse all CommandConstants.ts files to build:
 *   { "DATA_COMMANDS": { "LIST": "data/list", "CREATE": "data/create", ... }, ... }
 */
function buildConstantsMap(rootPath: string): Map<string, Record<string, string>> {
  const map = new Map<string, Record<string, string>>();

  const constantFiles = glob.sync(join(rootPath, '**/*CommandConstants.ts'), {
    ignore: [join(rootPath, 'node_modules/**'), join(rootPath, 'dist/**')],
  });

  for (const filePath of constantFiles) {
    const content = readFileSync(filePath, 'utf-8');

    // Match: export const SOME_COMMANDS = { ... } as const;
    const blockRegex = /export\s+const\s+(\w+_COMMANDS)\s*=\s*\{([^}]+)\}\s*as\s*const/gs;
    let blockMatch;
    while ((blockMatch = blockRegex.exec(content)) !== null) {
      const constName = blockMatch[1]; // e.g., "DATA_COMMANDS"
      const body = blockMatch[2];
      const entries: Record<string, string> = {};

      // Match: KEY: 'command/path',
      const entryRegex = /(\w+)\s*:\s*'([^']+)'/g;
      let entryMatch;
      while ((entryMatch = entryRegex.exec(body)) !== null) {
        entries[entryMatch[1]] = entryMatch[2];
      }

      if (Object.keys(entries).length > 0) {
        map.set(constName, entries);
      }
    }
  }

  return map;
}

/**
 * Scan all command Types files for executor exports.
 * Returns: command-path → ExecutorMapping
 */
function buildExecutorMap(rootPath: string): Map<string, ExecutorMapping> {
  const map = new Map<string, ExecutorMapping>();

  const typesFiles = glob.sync(join(rootPath, 'commands/**/shared/*Types.ts'), {
    ignore: [join(rootPath, 'node_modules/**'), join(rootPath, 'dist/**')],
  });

  for (const filePath of typesFiles) {
    const content = readFileSync(filePath, 'utf-8');
    const relPath = relative(rootPath, filePath).replace(/\.ts$/, '');

    // Find executor objects by locating commandName properties and working backwards.
    // Must ensure the executor has an execute() method (not just any const with commandName).
    const commandNameRegex = /commandName:\s*'([^']+)'\s*as\s*const/g;
    let match;
    while ((match = commandNameRegex.exec(content)) !== null) {
      const commandPath = match[1];
      const matchEnd = match.index + match[0].length;

      // Search backwards from commandName to find the NEAREST export const declaration
      const textBefore = content.substring(0, match.index);
      const lastExportIdx = textBefore.lastIndexOf('export const ');
      if (lastExportIdx === -1) continue;

      const afterExport = textBefore.substring(lastExportIdx);
      const nameMatch = afterExport.match(/^export\s+const\s+(\w+)\s*=\s*\{/);
      if (!nameMatch) continue;

      const executorName = nameMatch[1];

      // Verify this object has an execute() method (not just any const with commandName)
      const objectText = content.substring(lastExportIdx, matchEnd + 50);
      if (!/\bexecute\s*[<(]/.test(objectText)) continue;

      // Check if execute method has a generic type parameter
      const hasGenericResult = /execute\s*<\s*\w+/.test(objectText);

      map.set(commandPath, {
        executorName,
        importModule: relPath,
        hasGenericResult,
      });
    }
  }

  return map;
}

// ============================================================================
// MIGRATION LOGIC
// ============================================================================

/**
 * Resolve a command reference to a command path.
 * Handles DATA_COMMANDS.LIST, FILE_COMMANDS.LOAD, 'data/list', etc.
 */
function resolveCommandPath(
  ref: string,
  constantsMap: Map<string, Record<string, string>>
): string | null {
  // String literal: 'data/list' or "data/list"
  const stringMatch = ref.match(/^['"]([^'"]+)['"]$/);
  if (stringMatch) return stringMatch[1];

  // Constant reference: DATA_COMMANDS.LIST
  const constMatch = ref.match(/^(\w+)\.(\w+)$/);
  if (constMatch) {
    const group = constantsMap.get(constMatch[1]);
    if (group) return group[constMatch[2]] || null;
  }

  return null;
}

function migrateFile(
  filePath: string,
  rootPath: string,
  executorMap: Map<string, ExecutorMapping>,
  constantsMap: Map<string, Record<string, string>>,
  dryRun: boolean
): FileReport {
  const relPath = relative(rootPath, filePath);
  const report: FileReport = {
    filePath: relPath,
    replacements: 0,
    executorsUsed: [],
    errors: [],
  };

  const content = readFileSync(filePath, 'utf-8');

  // We need a robust pattern that handles:
  //   Commands.execute<ParamsType, ResultType>(COMMAND_REF,
  //   Commands.execute<ParamsType, ResultType<EntityType>>(COMMAND_REF,
  //   Commands.execute(COMMAND_REF,
  //
  // Strategy: find each "Commands.execute" occurrence, then manually parse
  // the generics and command reference from that position.

  const needle = 'Commands.execute';
  const replacements: Array<{
    start: number;
    end: number;
    text: string;
    executor: ExecutorMapping;
  }> = [];

  let searchFrom = 0;
  while (true) {
    const idx = content.indexOf(needle, searchFrom);
    if (idx === -1) break;
    searchFrom = idx + needle.length;

    // Parse what follows Commands.execute
    let pos = idx + needle.length;
    let entityGeneric: string | null = null;

    // Check for generics: <...>
    if (content[pos] === '<') {
      // Find matching > by counting angle brackets
      let depth = 1;
      pos++; // skip opening <
      const genericStart = pos;
      while (pos < content.length && depth > 0) {
        if (content[pos] === '<') depth++;
        else if (content[pos] === '>') depth--;
        pos++;
      }
      // genericContent = everything inside the outer < >
      const genericContent = content.substring(genericStart, pos - 1);

      // Extract entity generic from pattern like "DataListParams, DataListResult<UserEntity>"
      const entityMatch = genericContent.match(/,\s*\w+<(\w+)>/);
      if (entityMatch) {
        entityGeneric = entityMatch[1];
      }
    }

    // Now expect ( followed by command reference
    // Skip whitespace
    while (pos < content.length && /\s/.test(content[pos])) pos++;
    if (content[pos] !== '(') continue;
    pos++; // skip (

    // Skip whitespace
    while (pos < content.length && /\s/.test(content[pos])) pos++;

    // Read command reference: either a constant (WORD.WORD) or string literal ('...')
    let commandRef = '';
    const refStart = pos;

    if (content[pos] === "'" || content[pos] === '"') {
      // String literal
      const quote = content[pos];
      pos++;
      while (pos < content.length && content[pos] !== quote) pos++;
      pos++; // skip closing quote
      commandRef = content.substring(refStart, pos);
    } else if (/\w/.test(content[pos])) {
      // Identifier (constant like DATA_COMMANDS.LIST)
      while (pos < content.length && /[\w.]/.test(content[pos])) pos++;
      commandRef = content.substring(refStart, pos);
    } else {
      continue; // Can't parse
    }

    // Skip whitespace + comma
    while (pos < content.length && /\s/.test(content[pos])) pos++;
    if (content[pos] !== ',') continue;
    pos++; // skip comma
    while (pos < content.length && /\s/.test(content[pos])) pos++;

    // Resolve command path
    const commandPath = resolveCommandPath(commandRef, constantsMap);
    if (!commandPath) {
      // Not a known constant, skip
      continue;
    }

    // Find executor
    const executor = executorMap.get(commandPath);
    if (!executor) {
      report.errors.push(`No executor for '${commandPath}' (ref: ${commandRef})`);
      continue;
    }

    // Build replacement text
    let replacement: string;
    if (entityGeneric && executor.hasGenericResult && entityGeneric !== 'BaseEntity' && entityGeneric !== 'RecordData') {
      replacement = `${executor.executorName}.execute<${entityGeneric}>(`;
    } else {
      replacement = `${executor.executorName}.execute(`;
    }

    replacements.push({
      start: idx,
      end: pos,
      text: replacement,
      executor,
    });
  }

  if (replacements.length === 0) return report;

  // Apply replacements in reverse order
  let modified = content;
  for (let i = replacements.length - 1; i >= 0; i--) {
    const r = replacements[i];
    modified = modified.substring(0, r.start) + r.text + modified.substring(r.end);
  }

  // Collect unique executors needed
  const neededExecutors = new Map<string, ExecutorMapping>();
  for (const r of replacements) {
    neededExecutors.set(r.executor.executorName, r.executor);
  }

  // Add imports for executors not already imported
  for (const [name, executor] of neededExecutors) {
    const importCheck = new RegExp(`\\b${name}\\b.*from\\s+['"]`);
    if (importCheck.test(modified)) continue; // already imported

    // Calculate relative import path
    const fromDir = dirname(join(rootPath, relPath));
    const toModule = join(rootPath, executor.importModule);
    let relativePath = relative(fromDir, toModule);
    if (!relativePath.startsWith('.')) {
      relativePath = './' + relativePath;
    }

    // Find insertion point: after last import statement
    const lastImportMatches = [...modified.matchAll(/from\s+['"][^'"]+['"];\s*\n/g)];
    if (lastImportMatches.length > 0) {
      const lastImport = lastImportMatches[lastImportMatches.length - 1];
      const insertPos = lastImport.index! + lastImport[0].length;
      const importStatement = `import { ${name} } from '${relativePath}';\n`;
      modified = modified.substring(0, insertPos) + importStatement + modified.substring(insertPos);
    }

    report.executorsUsed.push(name);
  }

  report.replacements = replacements.length;

  if (!dryRun && modified !== content) {
    writeFileSync(filePath, modified, 'utf-8');
  }

  return report;
}

// ============================================================================
// MAIN
// ============================================================================

function main() {
  const rootPath = join(__dirname, '..');
  const dryRun = process.argv.includes('--dry-run');

  console.log('🔄 Migrating Commands.execute() calls to static executors\n');
  if (dryRun) console.log('   ⚡ DRY RUN — no files will be modified\n');

  // Step 1: Build constants map (DATA_COMMANDS.LIST → 'data/list', etc.)
  const constantsMap = buildConstantsMap(rootPath);
  let totalConstants = 0;
  for (const [group, entries] of constantsMap) {
    const count = Object.keys(entries).length;
    totalConstants += count;
    console.log(`   📋 ${group}: ${count} constants`);
  }
  console.log(`   Total: ${totalConstants} command constants\n`);

  // Step 2: Build executor map (command-path → executor info)
  const executorMap = buildExecutorMap(rootPath);
  console.log(`   🎯 ${executorMap.size} command executors available\n`);

  // Step 3: Find all TypeScript files to migrate
  const allFiles = glob.sync(join(rootPath, '**/*.ts'), {
    ignore: [
      join(rootPath, 'node_modules/**'),
      join(rootPath, 'dist/**'),
      // Don't modify the executor definitions themselves
      join(rootPath, 'commands/**/shared/*Types.ts'),
      // Don't modify command constant files
      join(rootPath, '**/*CommandConstants.ts'),
      // Don't modify generator templates (contain template literals, not real calls)
      join(rootPath, 'generator/**'),
    ],
  });

  console.log(`📄 Scanning ${allFiles.length} files...\n`);

  let totalReplacements = 0;
  let filesModified = 0;
  const allExecutors = new Set<string>();
  const allErrors: string[] = [];

  for (const filePath of allFiles) {
    const report = migrateFile(filePath, rootPath, executorMap, constantsMap, dryRun);
    if (report.replacements > 0) {
      filesModified++;
      totalReplacements += report.replacements;
      report.executorsUsed.forEach(e => allExecutors.add(e));
      console.log(`  ✅ ${report.filePath}: ${report.replacements} calls → ${report.executorsUsed.join(', ')}`);
    }
    if (report.errors.length > 0) {
      for (const e of report.errors) {
        allErrors.push(`${report.filePath}: ${e}`);
      }
    }
  }

  console.log(`\n📊 Summary:`);
  console.log(`   Files modified: ${filesModified}`);
  console.log(`   Calls migrated: ${totalReplacements}`);
  console.log(`   Unique executors: ${allExecutors.size} (${[...allExecutors].sort().join(', ')})`);

  if (allErrors.length > 0) {
    console.log(`\n⚠️  Warnings (${allErrors.length}):`);
    for (const e of allErrors) {
      console.log(`   ${e}`);
    }
  }

  if (dryRun) {
    console.log('\n⚡ DRY RUN complete — no files were modified');
  } else {
    console.log('\n✅ Migration complete — run npm run build:ts to verify');
  }
}

main();
