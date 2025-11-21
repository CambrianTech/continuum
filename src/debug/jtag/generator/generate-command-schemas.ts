/**
 * Command Parameter Schema Generator
 *
 * Automatically extracts command parameter schemas from TypeScript interface definitions
 * and generates a registry file that the 'list' command can use for dynamic discovery.
 *
 * This eliminates the need for manual schema maintenance in ListCommand.ts and enables
 * PersonaUsers to immediately discover new command syntax without any manual steps.
 *
 * **How it works:**
 * 1. Glob for all command Type files (commands slash star star slash shared slash star Types.ts)
 * 2. For each file, find the interface that extends CommandParams
 * 3. Extract JSDoc comments and property types
 * 4. Generate CommandSignature objects with params field
 * 5. Write to generated-command-schemas.json
 *
 * **Integration:**
 * - Runs automatically via prebuild script in package.json
 * - ListCommand.ts reads generated-command-schemas.json at startup
 * - Help command queries list command dynamically to get full syntax
 *
 * **Why this matters:**
 * - Zero manual maintenance - add command, run build, it appears in help
 * - PersonaUsers can discover syntax immediately after deployment
 * - Single source of truth (TypeScript interfaces)
 * - Type-safe by design (can't get out of sync)
 */

import { readFileSync, writeFileSync, readdirSync, statSync, existsSync } from 'fs';
import { join, relative } from 'path';
import * as glob from 'glob';

// ============================================================================
// TYPES
// ============================================================================

interface CommandParamDef {
  type: string;
  required: boolean;
  description?: string;
}

interface CommandSchema {
  name: string;
  description: string;
  params: Record<string, CommandParamDef>;
}

interface GeneratedSchemas {
  generated: string;
  version: string;
  commands: CommandSchema[];
}

// ============================================================================
// SCHEMA EXTRACTOR
// ============================================================================

class CommandSchemaGenerator {
  private rootPath: string;
  private schemas: CommandSchema[] = [];

  constructor(rootPath: string) {
    this.rootPath = rootPath;
  }

  /**
   * Main entry point - discover and extract all command schemas
   */
  generate(): GeneratedSchemas {
    console.log('🔍 Discovering command Type files...');

    // Find all *Types.ts files in commands/**/shared/
    const pattern = join(this.rootPath, 'commands/**/shared/*Types.ts');
    const files = glob.sync(pattern);

    console.log(`📄 Found ${files.length} Type files`);

    for (const filePath of files) {
      try {
        const schema = this.extractSchemaFromFile(filePath);
        if (schema) {
          this.schemas.push(schema);
          console.log(`  ✅ ${schema.name} (${Object.keys(schema.params).length} params)`);
        }
      } catch (error) {
        console.error(`  ❌ Failed to extract from ${filePath}:`, error);
      }
    }

    console.log(`\n✅ Generated ${this.schemas.length} command schemas`);

    return {
      generated: new Date().toISOString(),
      version: '1.0.0',
      commands: this.schemas
    };
  }

  /**
   * Extract command schema from a single *Types.ts file
   */
  private extractSchemaFromFile(filePath: string): CommandSchema | null {
    const content = readFileSync(filePath, 'utf-8');
    const relativePath = relative(this.rootPath, filePath);

    // Extract command name from path: commands/shell/execute/shared/ShellExecuteTypes.ts → shell/execute
    const commandNameMatch = relativePath.match(/commands\/(.+?)\/shared\/\w+Types\.ts$/);
    if (!commandNameMatch) {
      console.warn(`  ⚠️ Could not extract command name from: ${relativePath}`);
      return null;
    }

    const commandName = commandNameMatch[1];

    // Find the Params interface (e.g., "export interface ShellExecuteParams extends CommandParams")
    const paramsInterfaceRegex = /export\s+interface\s+(\w+Params)\s+extends\s+CommandParams\s*\{([^}]+)\}/s;
    const paramsMatch = content.match(paramsInterfaceRegex);

    if (!paramsMatch) {
      console.warn(`  ⚠️ No Params interface found in: ${relativePath}`);
      return null;
    }

    const interfaceBody = paramsMatch[2];

    // Extract description from JSDoc comment before the interface
    const description = this.extractDescription(content, paramsMatch.index!);

    // Extract parameters from interface body
    const params = this.extractParams(interfaceBody, content, paramsMatch.index!);

    return {
      name: commandName,
      description: description || `${commandName} command`,
      params
    };
  }

  /**
   * Extract description from JSDoc comment
   */
  private extractDescription(content: string, interfaceStart: number): string {
    const before = content.substring(0, interfaceStart);
    const jsdocMatch = before.match(/\/\*\*\s*\n\s*\*\s*(.+?)\n\s*\*\//s);
    if (jsdocMatch) {
      return jsdocMatch[1].trim();
    }
    return '';
  }

  /**
   * Extract parameters from interface body
   */
  private extractParams(interfaceBody: string, fullContent: string, interfaceStart: number): Record<string, CommandParamDef> {
    const params: Record<string, CommandParamDef> = {};

    // Match property definitions: propertyName?: type;
    const propertyRegex = /^\s*(?:readonly\s+)?(\w+)(\?)?:\s*([^;]+);/gm;
    let match;

    while ((match = propertyRegex.exec(interfaceBody)) !== null) {
      const [, propName, optional, propType] = match;

      // Skip inherited properties from CommandParams
      if (['context', 'sessionId', 'contextId'].includes(propName)) {
        continue;
      }

      // Extract JSDoc comment for this property
      const propDescription = this.extractPropertyDescription(interfaceBody, match.index);

      params[propName] = {
        type: this.simplifyType(propType.trim()),
        required: !optional,
        description: propDescription || `${propName} parameter`
      };
    }

    return params;
  }

  /**
   * Extract property description from JSDoc comment
   */
  private extractPropertyDescription(content: string, propStart: number): string {
    const before = content.substring(0, propStart);
    const lines = before.split('\n');

    // Look backwards for JSDoc comment
    for (let i = lines.length - 1; i >= 0; i--) {
      const line = lines[i].trim();

      // Found comment line
      if (line.startsWith('*') && !line.startsWith('*/') && !line.startsWith('/**')) {
        const commentText = line.replace(/^\*\s*/, '').trim();
        if (commentText) {
          return commentText;
        }
      }

      // Stop at empty lines or start of JSDoc
      if (!line || line === '/**' || line === '*/') {
        break;
      }
    }

    return '';
  }

  /**
   * Simplify TypeScript type to basic JSON type
   */
  private simplifyType(tsType: string): string {
    // Remove readonly, optional, etc.
    const cleaned = tsType.replace(/readonly\s+/g, '').trim();

    // Map TypeScript types to JSON Schema types
    if (cleaned === 'string' || cleaned.includes('string')) {
      return 'string';
    }
    if (cleaned === 'number' || cleaned.includes('number')) {
      return 'number';
    }
    if (cleaned === 'boolean') {
      return 'boolean';
    }
    if (cleaned.startsWith('Array<') || cleaned.includes('[]')) {
      return 'array';
    }
    if (cleaned.startsWith('Record<') || cleaned.startsWith('{')) {
      return 'object';
    }

    // Default to string for complex types
    return 'string';
  }
}

// ============================================================================
// MAIN EXECUTION
// ============================================================================

function main() {
  console.log('🏭 Command Parameter Schema Generator\n');

  const rootPath = process.cwd();
  console.log(`📁 Root path: ${rootPath}\n`);

  // Generate schemas
  const generator = new CommandSchemaGenerator(rootPath);
  const result = generator.generate();

  // Write to output file
  const outputPath = join(rootPath, 'generated-command-schemas.json');
  writeFileSync(outputPath, JSON.stringify(result, null, 2), 'utf-8');

  console.log(`\n💾 Wrote schemas to: ${outputPath}`);
  console.log(`📊 Total commands: ${result.commands.length}`);
  console.log('✨ Done!\n');
}

// Run if executed directly
if (require.main === module) {
  try {
    main();
  } catch (error) {
    console.error('❌ Schema generation failed:', error);
    process.exit(1);
  }
}
