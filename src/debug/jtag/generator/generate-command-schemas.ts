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
        const schemas = this.extractSchemasFromFile(filePath);
        for (const schema of schemas) {
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
   * Extract ALL command schemas from a single *Types.ts file
   * (Handles multi-command files like WallTypes.ts with WallWriteParams, WallReadParams, WallListParams)
   */
  private extractSchemasFromFile(filePath: string): CommandSchema[] {
    const content = readFileSync(filePath, 'utf-8');
    const relativePath = relative(this.rootPath, filePath);

    // Extract base command path from file path: commands/wall/shared/WallTypes.ts → wall
    const commandPathMatch = relativePath.match(/commands\/(.+?)\/shared\/\w+Types\.ts$/);
    if (!commandPathMatch) {
      console.warn(`  ⚠️ Could not extract command path from: ${relativePath}`);
      return [];
    }

    const basePath = commandPathMatch[1];

    // Find ALL *Params interfaces that extend CommandParams (or base interfaces that do)
    // Use global regex to find all matches
    const paramsInterfaceRegex = /export\s+interface\s+(\w+Params)\s+extends\s+(\w+)\s*\{([^}]+)\}/gs;
    const schemas: CommandSchema[] = [];
    let match;

    while ((match = paramsInterfaceRegex.exec(content)) !== null) {
      const interfaceName = match[1];
      const parentInterface = match[2];
      const interfaceBody = match[3];

      // Derive command name from interface name
      // WallWriteParams → wall/write, WallListParams → wall/list, PingParams → ping
      const commandName = this.deriveCommandName(interfaceName, basePath);

      // Check if this extends CommandParams directly or through an intermediate interface
      let allParams: Record<string, CommandParamDef> = {};

      if (parentInterface !== 'CommandParams') {
        // Double inheritance - need to find parent interface in same file
        const parentParams = this.extractParentParams(content, parentInterface);
        if (parentParams === null) {
          console.warn(`  ⚠️ Parent interface ${parentInterface} not found or doesn't extend CommandParams: ${interfaceName}`);
          continue;
        }
        // Merge parent params
        allParams = { ...parentParams };
      }

      // Extract description from JSDoc comment before the interface
      const description = this.extractDescription(content, match.index);

      // Extract parameters from this interface body and merge with parent
      const params = this.extractParams(interfaceBody, content, match.index);
      allParams = { ...allParams, ...params };

      schemas.push({
        name: commandName,
        description: description || `${commandName} command`,
        params: allParams
      });
    }

    if (schemas.length === 0) {
      console.warn(`  ⚠️ No Params interfaces found in: ${relativePath}`);
    }

    return schemas;
  }

  /**
   * Derive command name from Params interface name and base path
   * Examples:
   *   WallWriteParams + "wall" → "wall/write"
   *   WallListParams + "wall" → "wall/list"
   *   PingParams + "ping" → "ping"
   *   ScreenshotParams + "screenshot" → "screenshot"
   */
  private deriveCommandName(interfaceName: string, basePath: string): string {
    // Remove "Params" suffix: WallWriteParams → WallWrite
    const withoutParams = interfaceName.replace(/Params$/, '');

    // Convert PascalCase to kebab-case: WallWrite → wall-write
    const kebabCase = withoutParams
      .replace(/([a-z])([A-Z])/g, '$1-$2')
      .toLowerCase();

    // If kebab starts with basePath, extract subcommand
    // "wall-write" with basePath "wall" → "wall/write"
    // "ping" with basePath "ping" → "ping"
    if (kebabCase.startsWith(basePath + '-')) {
      const subcommand = kebabCase.substring(basePath.length + 1);
      return `${basePath}/${subcommand}`;
    }

    return kebabCase;
  }

  /**
   * Extract params from a parent interface (recursive, supports N levels of inheritance)
   * Returns null if parent interface doesn't extend CommandParams at any level OR contain required fields
   */
  private extractParentParams(content: string, parentInterfaceName: string, visited: Set<string> = new Set()): Record<string, CommandParamDef> | null {
    // Prevent infinite loops from circular inheritance
    if (visited.has(parentInterfaceName)) {
      return null;
    }
    visited.add(parentInterfaceName);

    // Base case: if parent is CommandParams, we've reached the root
    if (parentInterfaceName === 'CommandParams') {
      return {}; // No params from CommandParams itself (context, sessionId handled separately)
    }

    // Look for the parent interface definition - with or without extends clause
    // Pattern 1: export interface Foo extends Bar { ... }
    // Pattern 2: export interface Foo { ... }
    const parentWithExtendsRegex = new RegExp(
      `export\\s+interface\\s+${parentInterfaceName}\\s+extends\\s+(\\w+)\\s*\\{([^}]+)\\}`,
      's'
    );
    const parentStandaloneRegex = new RegExp(
      `export\\s+interface\\s+${parentInterfaceName}\\s*\\{([^}]+)\\}`,
      's'
    );

    let parentMatch = content.match(parentWithExtendsRegex);
    let grandparentInterface: string | null = null;
    let parentBody: string;

    if (parentMatch) {
      // Has extends clause
      grandparentInterface = parentMatch[1];
      parentBody = parentMatch[2];
    } else {
      // Try standalone interface
      const standaloneMatch = content.match(parentStandaloneRegex);
      if (!standaloneMatch) {
        return null;
      }
      parentBody = standaloneMatch[1];
      grandparentInterface = null; // No grandparent
    }

    // Extract params from this parent's body
    const parentParams = this.extractParams(parentBody, content, 0);

    // Check if this interface has required fields (context and sessionId)
    const hasContext = parentBody.includes('context:');
    const hasSessionId = parentBody.includes('sessionId:');

    if (hasContext && hasSessionId) {
      // This interface is compatible with CommandParams even if it doesn't formally extend it
      return parentParams;
    }

    // If no required fields, check if it extends something else
    if (grandparentInterface) {
      const grandparentParams = this.extractParentParams(content, grandparentInterface, visited);
      if (grandparentParams === null) {
        return null;
      }
      // Merge grandparent params with parent params
      return { ...grandparentParams, ...parentParams };
    }

    // No extends, no required fields = invalid
    return null;
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
    // IMPORTANT: Check arrays FIRST before checking for primitives
    // Otherwise "string[]" matches "includes('string')" and becomes "string" instead of "array"
    if (cleaned.startsWith('Array<') || cleaned.includes('[]')) {
      return 'array';
    }
    if (cleaned.startsWith('Record<') || cleaned.startsWith('{')) {
      return 'object';
    }
    if (cleaned === 'string' || cleaned.includes('string')) {
      return 'string';
    }
    if (cleaned === 'number' || cleaned.includes('number')) {
      return 'number';
    }
    if (cleaned === 'boolean') {
      return 'boolean';
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
