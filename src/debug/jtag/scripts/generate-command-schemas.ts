/**
 * Generate command schemas from TypeScript types
 *
 * Uses TypeScriptCompiler to properly resolve cross-file inheritance.
 * Generates command-schemas.json for use by help command.
 */

import * as fs from 'fs';
import * as path from 'path';
import { TypeScriptCompiler } from '../system/typescript/shared/TypeScriptCompiler';

interface ParamSchema {
  type: string;
  required: boolean;
  description?: string;
}

interface CommandSchema {
  name: string;
  description: string;
  params: Record<string, ParamSchema>;
}

const COMMANDS_DIR = path.join(__dirname, '../commands');
const OUTPUT_FILE = path.join(__dirname, '../generated/command-schemas.json');
const ROOT_DIR = path.join(__dirname, '..');

function findAllTypeFiles(dir: string): string[] {
  const files: string[] = [];

  function scan(currentDir: string) {
    if (!fs.existsSync(currentDir)) return;

    const entries = fs.readdirSync(currentDir, { withFileTypes: true });
    for (const entry of entries) {
      const fullPath = path.join(currentDir, entry.name);

      // Skip node_modules, dist, etc.
      if (entry.name === 'node_modules' || entry.name === 'dist' || entry.name.startsWith('.')) {
        continue;
      }

      if (entry.isDirectory()) {
        scan(fullPath);
      } else if (entry.name.endsWith('Types.ts') && fullPath.includes('/shared/')) {
        files.push(fullPath);
      }
    }
  }

  scan(dir);
  return files;
}

function extractSchemaFromFile(
  compiler: TypeScriptCompiler,
  filePath: string
): CommandSchema | null {
  // Extract command name from file path (e.g., commands/data/read/shared/DataReadTypes.ts -> data/read)
  const parts = filePath.split('/commands/')[1].split('/');
  const commandName = parts.slice(0, -2).join('/'); // Remove 'shared' and filename

  // Find *Params interface in this file
  const content = fs.readFileSync(filePath, 'utf-8');
  const paramsMatch = content.match(/export interface (\w+Params)/);

  if (!paramsMatch) {
    return null;
  }

  const paramsInterfaceName = paramsMatch[1];

  // Use TypeScriptCompiler to get full interface info (with inheritance)
  const interfaceInfo = compiler.getInterfaceInfo(paramsInterfaceName, filePath);

  if (!interfaceInfo) {
    console.warn(`⚠️  Could not extract interface info for ${paramsInterfaceName} in ${filePath}`);
    return null;
  }

  // Convert to CommandSchema format
  const params: Record<string, ParamSchema> = {};

  for (const prop of interfaceInfo.properties) {
    // Skip context and sessionId (added automatically by JTAG)
    if (prop.name === 'context' || prop.name === 'sessionId') {
      continue;
    }

    // Skip backend (internal routing parameter)
    if (prop.name === 'backend') {
      continue;
    }

    params[prop.name] = {
      type: prop.type,
      required: prop.required,
      description: prop.description
    };
  }

  // Extract description from JSDoc comments if available
  const descriptionMatch = content.match(/\/\*\*\s*\n\s*\*\s*(.+?)\n/);
  const description = descriptionMatch ? descriptionMatch[1].trim() : `${commandName} command`;

  return {
    name: commandName,
    description,
    params
  };
}

function main() {
  console.log('🔧 Generating command schemas...');

  // Initialize TypeScript compiler with proper module resolution
  const compiler = new TypeScriptCompiler({ rootDir: ROOT_DIR });

  // Find all *Types.ts files in commands directory
  const typeFiles = findAllTypeFiles(COMMANDS_DIR);
  console.log(`📁 Found ${typeFiles.length} type files`);

  // Extract schemas from each file
  const schemas: Record<string, CommandSchema> = {};
  let successCount = 0;

  for (const file of typeFiles) {
    const schema = extractSchemaFromFile(compiler, file);
    if (schema) {
      schemas[schema.name] = schema;
      const paramCount = Object.keys(schema.params).length;
      console.log(`✓ ${schema.name} (${paramCount} params)`);
      successCount++;
    }
  }

  // Ensure generated directory exists
  const generatedDir = path.dirname(OUTPUT_FILE);
  if (!fs.existsSync(generatedDir)) {
    fs.mkdirSync(generatedDir, { recursive: true });
  }

  // Write schemas to file
  fs.writeFileSync(OUTPUT_FILE, JSON.stringify(schemas, null, 2), 'utf-8');

  console.log(`\n✅ Generated ${successCount} command schemas`);
  console.log(`📝 Output: ${OUTPUT_FILE}`);
}

main();
