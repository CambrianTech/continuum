/**
 * Schema Generate Command Types
 *
 * Generates JSON schemas from TypeScript interfaces using the TypeScript compiler.
 * Properly resolves cross-file inheritance.
 *
 * Usage:
 *   ./jtag schema/generate --pattern="*Params" --output="schemas.json"
 *   ./jtag schema/generate --interface="DataReadParams" --file="commands/data/read/shared/DataReadTypes.ts"
 */

import type { CommandParams, JTAGPayload } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';

export interface SchemaGenerateParams extends CommandParams {
  /**
   * Pattern to match interface names (e.g., "*Params", "*Result")
   * If not provided, must specify interface + file
   */
  readonly pattern?: string;

  /**
   * Specific interface name to generate schema for
   */
  readonly interface?: string;

  /**
   * File path containing the interface (required if interface specified)
   */
  readonly file?: string;

  /**
   * Output file path (defaults to stdout if not provided)
   */
  readonly output?: string;

  /**
   * Root directory to search (defaults to project root)
   */
  readonly rootDir?: string;
}

export interface SchemaGenerateResult extends JTAGPayload {
  readonly success: boolean;
  readonly timestamp: string;
  readonly error?: string;

  /**
   * Generated schemas
   */
  readonly schemas?: Record<string, InterfaceSchema>;

  /**
   * Number of schemas generated
   */
  readonly count?: number;

  /**
   * Output file path if written to file
   */
  readonly outputPath?: string;
}

/**
 * Schema representation of a TypeScript interface
 */
export interface InterfaceSchema {
  readonly name: string;
  readonly filePath: string;
  readonly description?: string;
  readonly properties: Record<string, PropertySchema>;
  readonly extends?: string[];
}

/**
 * Schema representation of an interface property
 */
export interface PropertySchema {
  readonly type: string;
  readonly required: boolean;
  readonly description?: string;
}

/**
 * Create a SchemaGenerateResult from params and data
 */
export function createSchemaGenerateResult(
  params: SchemaGenerateParams,
  differences: Omit<Partial<SchemaGenerateResult>, 'context' | 'sessionId'>
): SchemaGenerateResult {
  return transformPayload(params, {
    success: false,
    timestamp: new Date().toISOString(),
    ...differences
  });
}
