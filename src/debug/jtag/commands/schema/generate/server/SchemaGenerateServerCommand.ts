/**
 * Schema Generate Command - Server Implementation
 *
 * Uses TypeScriptCompiler foundation to generate schemas with proper cross-file inheritance.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type {
  SchemaGenerateParams,
  SchemaGenerateResult,
  InterfaceSchema,
  PropertySchema
} from '../shared/SchemaGenerateTypes';
import { createSchemaGenerateResult } from '../shared/SchemaGenerateTypes';
import { TypeScriptCompiler } from '../../../../system/typescript/shared/TypeScriptCompiler';
import * as fs from 'fs';
import * as path from 'path';

export class SchemaGenerateServerCommand extends CommandBase<SchemaGenerateParams, SchemaGenerateResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('schema/generate', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SchemaGenerateResult> {
    const schemaParams = params as SchemaGenerateParams;

    console.log(`🔧 SERVER: Generating schemas...`);

    try {
      // Determine root directory
      const rootDir = schemaParams.rootDir || path.join(__dirname, '../../../..');

      // Initialize TypeScript compiler
      const compiler = new TypeScriptCompiler({ rootDir });

      let schemas: Record<string, InterfaceSchema> = {};

      // Two modes: pattern matching or specific interface
      if (schemaParams.pattern) {
        // Pattern matching mode
        schemas = await this.generateFromPattern(compiler, schemaParams.pattern, rootDir);
      } else if (schemaParams.interface && schemaParams.file) {
        // Specific interface mode
        const schema = await this.generateFromInterface(
          compiler,
          schemaParams.interface,
          schemaParams.file
        );
        if (schema) {
          schemas[schema.name] = schema;
        }
      } else {
        return createSchemaGenerateResult(schemaParams, {
          success: false,
          error: 'Must provide either pattern or (interface + file)'
        });
      }

      // Write to file if output path provided
      let outputPath: string | undefined;
      if (schemaParams.output) {
        outputPath = path.isAbsolute(schemaParams.output)
          ? schemaParams.output
          : path.join(process.cwd(), schemaParams.output);

        fs.writeFileSync(outputPath, JSON.stringify(schemas, null, 2), 'utf-8');
        console.log(`✅ Wrote ${Object.keys(schemas).length} schemas to ${outputPath}`);
      } else {
        // Output to stdout
        console.log(JSON.stringify(schemas, null, 2));
      }

      return createSchemaGenerateResult(schemaParams, {
        success: true,
        schemas,
        count: Object.keys(schemas).length,
        outputPath
      });

    } catch (error) {
      console.error(`❌ SERVER: Failed to generate schemas:`, error);
      return createSchemaGenerateResult(schemaParams, {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
  }

  /**
   * Generate schemas for all interfaces matching a pattern
   */
  private async generateFromPattern(
    compiler: TypeScriptCompiler,
    pattern: string,
    searchDir: string
  ): Promise<Record<string, InterfaceSchema>> {
    const schemas: Record<string, InterfaceSchema> = {};

    // Convert glob pattern to regex (simple conversion)
    const regexPattern = new RegExp(
      pattern.replace(/\*/g, '.*').replace(/\?/g, '.')
    );

    // Find all matching interfaces
    const matches = compiler.findInterfaces(regexPattern, searchDir);

    console.log(`📋 Found ${matches.length} interfaces matching pattern "${pattern}"`);

    // Generate schema for each match
    for (const match of matches) {
      const schema = await this.generateFromInterface(
        compiler,
        match.name,
        match.filePath
      );

      if (schema) {
        schemas[schema.name] = schema;
      }
    }

    return schemas;
  }

  /**
   * Generate schema for a specific interface
   */
  private async generateFromInterface(
    compiler: TypeScriptCompiler,
    interfaceName: string,
    filePath: string
  ): Promise<InterfaceSchema | null> {
    const info = compiler.getInterfaceInfo(interfaceName, filePath);

    if (!info) {
      console.warn(`⚠️  Could not extract info for ${interfaceName} in ${filePath}`);
      return null;
    }

    // Convert to schema format
    const properties: Record<string, PropertySchema> = {};
    for (const prop of info.properties) {
      properties[prop.name] = {
        type: prop.type,
        required: prop.required,
        description: prop.description
      };
    }

    return {
      name: info.name,
      filePath: info.filePath,
      properties,
      extends: info.extends
    };
  }
}
