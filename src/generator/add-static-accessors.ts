#!/usr/bin/env npx tsx
/**
 * Surgical Static Accessor Injection
 *
 * Reads every command Types file missing a static accessor,
 * extracts the Params/Result interface names, and appends
 * a properly typed accessor block.
 *
 * Usage: npx tsx generator/add-static-accessors.ts [--dry-run]
 */

import * as fs from 'fs';
import * as path from 'path';
import { CommandAuditor } from './CommandAuditor';

const rootPath = process.cwd();
const dryRun = process.argv.includes('--dry-run');
const auditor = new CommandAuditor(rootPath);
const audit = auditor.audit();

// Find commands with Types files but no static accessor
const missing = audit.entries.filter(e => !e.hasStaticAccessor && e.hasTypesFile);
console.log(`\nFound ${missing.length} commands missing static accessors\n`);

let fixed = 0;
let skipped = 0;
const errors: string[] = [];

for (const entry of missing) {
  const sharedDir = path.join(entry.dirPath, 'shared');
  const typesFiles = fs.readdirSync(sharedDir).filter(f => f.endsWith('Types.ts'));
  if (typesFiles.length === 0) {
    errors.push(`${entry.commandName}: No Types file found`);
    skipped++;
    continue;
  }

  const typesPath = path.join(sharedDir, typesFiles[0]);
  const content = fs.readFileSync(typesPath, 'utf-8');

  // Extract Params interface name
  const paramsMatch = content.match(/export\s+interface\s+(\w+Params)\s+extends\s+CommandParams/);
  if (!paramsMatch) {
    errors.push(`${entry.commandName}: No Params interface extending CommandParams`);
    skipped++;
    continue;
  }
  const paramsName = paramsMatch[1];

  // Extract Result interface name
  const resultMatch = content.match(/export\s+interface\s+(\w+Result)(?:<[^>]+>)?\s+extends\s+(?:CommandResult|JTAGPayload)/);
  if (!resultMatch) {
    errors.push(`${entry.commandName}: No Result interface extending CommandResult/JTAGPayload`);
    skipped++;
    continue;
  }
  const resultName = resultMatch[1];

  // Check if Result has a generic param (like DataListResult<T extends BaseEntity>)
  const resultGenericMatch = content.match(new RegExp(`export\\s+interface\\s+${resultName}<([^>]+)>`));
  const hasGeneric = !!resultGenericMatch;

  // Build the accessor name from command name: "sentinel/run" → "SentinelRun"
  const accessorName = entry.commandName
    .split(/[\/\-_]/)
    .map(part => part.charAt(0).toUpperCase() + part.slice(1))
    .join('');

  // Check if this accessor name already exists (different detection than auditor)
  if (content.includes(`export const ${accessorName}`)) {
    skipped++;
    continue;
  }

  // Check if Commands import exists
  const hasCommandsImport = content.includes("from '../../../../system/core/shared/Commands'") ||
    content.includes("from '@system/core/shared/Commands'") ||
    content.includes("{ Commands }") || content.includes("{ Commands,");

  // Check if CommandInput import exists
  const hasCommandInputImport = content.includes('CommandInput');

  // Build the import additions needed
  const importsToAdd: string[] = [];

  if (!hasCommandsImport) {
    importsToAdd.push("import { Commands } from '@system/core/shared/Commands';");
  }
  if (!hasCommandInputImport) {
    // Need to add CommandInput to existing JTAGTypes import or add new one
    importsToAdd.push("import type { CommandInput } from '@system/core/types/JTAGTypes';");
  }

  // Build the accessor block
  let accessorBlock: string;
  if (hasGeneric) {
    // Generic result — accessor needs a type parameter
    // e.g., DataListResult<T extends BaseEntity>
    const genericParam = resultGenericMatch![1]; // "T extends BaseEntity"
    const genericName = genericParam.split(/\s/)[0]; // "T"
    const defaultType = genericParam.includes('extends')
      ? genericParam.split('extends')[1].trim()
      : 'unknown';
    accessorBlock = `
/**
 * ${accessorName} — Type-safe command executor
 *
 * Usage:
 *   import { ${accessorName} } from '...shared/${typesFiles[0].replace('.ts', '')}';
 *   const result = await ${accessorName}.execute({ ... });
 */
export const ${accessorName} = {
  execute<${genericParam} = ${defaultType}>(params: CommandInput<${paramsName}>): Promise<${resultName}<${genericName}>> {
    return Commands.execute<${paramsName}, ${resultName}<${genericName}>>('${entry.commandName}', params as Partial<${paramsName}>);
  },
  commandName: '${entry.commandName}' as const,
} as const;
`;
  } else {
    accessorBlock = `
/**
 * ${accessorName} — Type-safe command executor
 *
 * Usage:
 *   import { ${accessorName} } from '...shared/${typesFiles[0].replace('.ts', '')}';
 *   const result = await ${accessorName}.execute({ ... });
 */
export const ${accessorName} = {
  execute(params: CommandInput<${paramsName}>): Promise<${resultName}> {
    return Commands.execute<${paramsName}, ${resultName}>('${entry.commandName}', params as Partial<${paramsName}>);
  },
  commandName: '${entry.commandName}' as const,
} as const;
`;
  }

  if (dryRun) {
    console.log(`[DRY RUN] ${entry.commandName} → ${accessorName}`);
    console.log(`  Params: ${paramsName}, Result: ${resultName}${hasGeneric ? ' (generic)' : ''}`);
    if (importsToAdd.length > 0) console.log(`  Imports needed: ${importsToAdd.length}`);
    fixed++;
    continue;
  }

  // Apply changes
  let newContent = content;

  // Add missing imports at the top (after existing imports)
  if (importsToAdd.length > 0) {
    // Find the last import line
    const lines = newContent.split('\n');
    let lastImportIdx = -1;
    for (let i = 0; i < lines.length; i++) {
      if (lines[i].match(/^import\s/)) {
        lastImportIdx = i;
      }
    }
    if (lastImportIdx >= 0) {
      lines.splice(lastImportIdx + 1, 0, ...importsToAdd);
      newContent = lines.join('\n');
    } else {
      // No imports found, add at top
      newContent = importsToAdd.join('\n') + '\n' + newContent;
    }
  }

  // Append accessor block at end
  newContent = newContent.trimEnd() + '\n' + accessorBlock;

  fs.writeFileSync(typesPath, newContent, 'utf-8');
  console.log(`  ✅ ${entry.commandName} → ${accessorName}`);
  fixed++;
}

console.log(`\n${'─'.repeat(50)}`);
console.log(`  Fixed:   ${fixed}`);
console.log(`  Skipped: ${skipped}`);
if (errors.length > 0) {
  console.log(`  Errors:`);
  for (const err of errors) {
    console.log(`    ⚠️  ${err}`);
  }
}
console.log(`${'─'.repeat(50)}\n`);
