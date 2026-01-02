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
   * Extract interface body using proper brace counting
   *
   * The naive regex `([^}]+)` stops at the first `}`, breaking for nested objects like:
   * ```
   * interface Foo extends Bar {
   *   simple?: string;
   *   nested?: { x: number; };  // First } stops here!
   *   afterNested?: string;     // NEVER captured
   * }
   * ```
   *
   * This function properly counts braces to extract the full body.
   */
  private extractInterfaceBody(content: string, startIndex: number): string {
    let braceCount = 0;
    let inBody = false;
    let bodyStart = 0;

    for (let i = startIndex; i < content.length; i++) {
      const char = content[i];

      if (char === '{') {
        if (!inBody) {
          inBody = true;
          bodyStart = i + 1;
        }
        braceCount++;
      } else if (char === '}') {
        braceCount--;
        if (braceCount === 0 && inBody) {
          return content.substring(bodyStart, i);
        }
      }
    }

    return '';
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
    // FIXED: Use brace counting instead of naive ([^}]+) which stops at first }
    // This regex finds the interface START, then we use extractInterfaceBody for the body
    const paramsInterfaceStartRegex = /export\s+interface\s+(\w+Params)\s+extends\s+(\w+)\s*\{/g;
    const schemas: CommandSchema[] = [];

    // First pass: collect all interface names to detect multi-interface files
    const allInterfaceNames: string[] = [];
    const interfaceMatches: Array<{ interfaceName: string; parentInterface: string; index: number }> = [];
    let match;

    while ((match = paramsInterfaceStartRegex.exec(content)) !== null) {
      allInterfaceNames.push(match[1]);
      interfaceMatches.push({
        interfaceName: match[1],
        parentInterface: match[2],
        index: match.index
      });
    }

    // Second pass: process each interface
    for (const { interfaceName, parentInterface, index } of interfaceMatches) {
      // Use brace counting to extract full body including nested objects
      const interfaceBody = this.extractInterfaceBody(content, index);

      // Derive command name from interface name
      // WallWriteParams → wall/write, WallListParams → wall/list, PingParams → ping
      // Pass all interface names to detect subcommand patterns (only in multi-interface files)
      const commandName = this.deriveCommandName(interfaceName, basePath, allInterfaceNames);

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
      const description = this.extractDescription(content, index);

      // Extract parameters from this interface body and merge with parent
      const params = this.extractParams(interfaceBody, content, index);
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
   *
   * SIMPLIFIED APPROACH: Just use basePath directly since it comes from the file structure
   * which is the authoritative source of truth for command naming.
   *
   * Examples:
   *   TaskListParams + "workspace/task/list" → "workspace/task/list"
   *   TreeParams + "workspace/tree" → "workspace/tree"
   *   AIGenerateParams + "ai/generate" → "ai/generate"
   *   PingParams + "ping" → "ping"
   *   WallWriteParams + "wall" → "wall/write" (subcommand case - multi-interface file)
   *   WidgetStateDebugParams + "development/debug/widget-state" → "development/debug/widget-state"
   *     (NOT "development/debug/widget-state/debug" - "Debug" is naming convention, not subcommand)
   */
  private deriveCommandName(interfaceName: string, basePath: string, allInterfacesInFile: string[]): string {
    // Remove "Params" suffix: WallWriteParams → WallWrite, TaskListParams → TaskList
    const withoutParams = interfaceName.replace(/Params$/, '');

    // Convert PascalCase to kebab-case, handling acronyms correctly
    const kebabCase = withoutParams
      .replace(/([A-Z]+)([A-Z][a-z])/g, '$1-$2')  // AIGenerate → AI-Generate
      .replace(/([a-z])([A-Z])/g, '$1-$2')        // TaskList → Task-List
      .toLowerCase();

    // Get the last segment of basePath (workspace/task/list → list)
    const basePathSegments = basePath.split('/');
    const lastSegment = basePathSegments[basePathSegments.length - 1];

    // ONLY extract subcommands when there are MULTIPLE *Params interfaces in the same file
    // This prevents WidgetStateDebugParams from becoming "widget-state/debug"
    // when "Debug" is just a naming convention, not a subcommand indicator.
    //
    // Subcommands are for cases like WallTypes.ts containing:
    // - WallWriteParams → wall/write
    // - WallReadParams → wall/read
    // - WallListParams → wall/list
    if (allInterfacesInFile.length > 1 && kebabCase.startsWith(lastSegment + '-')) {
      const subcommand = kebabCase.substring(lastSegment.length + 1);
      return `${basePath}/${subcommand}`;
    }

    // Otherwise, just use the full basePath from the file structure
    // This handles all cases correctly:
    // - workspace/task/list (multi-level paths)
    // - interface/screenshot (two-level paths)
    // - ping (single-level paths)
    return basePath;
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
    // FIXED: Use brace counting instead of naive ([^}]+) which stops at first }
    // Pattern 1: export interface Foo extends Bar { ... }
    // Pattern 2: export interface Foo { ... }
    const parentWithExtendsStartRegex = new RegExp(
      `export\\s+interface\\s+${parentInterfaceName}\\s+extends\\s+(\\w+)\\s*\\{`
    );
    const parentStandaloneStartRegex = new RegExp(
      `export\\s+interface\\s+${parentInterfaceName}\\s*\\{`
    );

    let grandparentInterface: string | null = null;
    let parentBody: string;

    const withExtendsMatch = content.match(parentWithExtendsStartRegex);
    if (withExtendsMatch && withExtendsMatch.index !== undefined) {
      // Has extends clause - extract grandparent and use brace counting for body
      grandparentInterface = withExtendsMatch[1];
      parentBody = this.extractInterfaceBody(content, withExtendsMatch.index);
    } else {
      // Try standalone interface
      const standaloneMatch = content.match(parentStandaloneStartRegex);
      if (!standaloneMatch || standaloneMatch.index === undefined) {
        return null;
      }
      parentBody = this.extractInterfaceBody(content, standaloneMatch.index);
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
