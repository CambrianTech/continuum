/**
 * Data Schema Command - Entity Schema Introspection
 *
 * Provides runtime schema information from entity decorators
 * Allows callers to understand entity structure, constraints, and validation rules
 */

import type { CommandParams, JTAGPayload, JTAGContext, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { FieldMetadata } from '../../../../system/data/decorators/FieldDecorators';
import { Commands } from '../../../../system/core/shared/Commands';

/** Introspect an entity collection's schema at runtime, returning field types, constraints, indexes, optional examples, SQL, and data validation. */
export interface DataSchemaParams extends CommandParams {
  readonly collection: string; // Entity collection name to get schema for
  readonly examples?: boolean; // Include example JSON objects
  readonly sql?: boolean; // Include SQL CREATE statements
  readonly validateData?: Record<string, unknown>; // JSON data to validate against schema
}

/**
 * Enhanced field information including decorator metadata
 */
export interface SchemaField {
  fieldName: string;
  fieldType: 'primary' | 'foreign_key' | 'date' | 'enum' | 'text' | 'json' | 'blob' | 'number' | 'boolean';
  required: boolean; // Derived from !nullable
  nullable: boolean;
  index?: boolean;
  unique?: boolean;
  maxLength?: number;
  default?: any;
  references?: string; // For foreign keys: 'User.userId'
}

/**
 * Example JSON objects for entity creation
 */
export interface EntityExamples {
  minimal: Record<string, any>; // Only required fields
  complete: Record<string, any>; // All fields with realistic values
  description: string; // Explanation of the examples
}

/**
 * SQL statements for entity
 */
export interface EntitySQL {
  createTable: string; // CREATE TABLE statement
  indexes: string[]; // CREATE INDEX statements
  foreignKeys: string[]; // ALTER TABLE ADD FOREIGN KEY statements
}

/**
 * Validation result for JSON data against entity schema
 */
export interface ValidationResult {
  readonly valid: boolean;
  readonly error?: string;
  readonly validatedEntity?: Record<string, unknown>;
}

/**
 * Complete entity schema information
 */
export interface EntitySchema {
  collection: string;
  entityClass: string;
  fields: SchemaField[];
  primaryKey?: string;
  indexes: string[]; // Fields with index: true
  foreignKeys: Array<{
    field: string;
    references: string;
  }>;
  requiredFields: string[]; // Fields where nullable: false
  examples?: EntityExamples; // Example JSON objects (when requested)
  sql?: EntitySQL; // SQL statements (when requested)
}

/**
 * Response from data/schema command
 */
export interface DataSchemaResult extends JTAGPayload {
  readonly success: boolean;
  readonly schema?: EntitySchema;
  readonly collection: string;
  readonly timestamp: string;
  readonly error?: string;
  readonly validation?: ValidationResult; // Present when validateData was provided
}

export const createDataSchemaParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataSchemaParams, 'context' | 'sessionId'>
): DataSchemaParams => createPayload(context, sessionId, data);

export const createDataSchemaResultFromParams = (
  params: DataSchemaParams,
  differences: Omit<Partial<DataSchemaResult>, 'context' | 'sessionId'>
): DataSchemaResult => transformPayload(params, {
  success: false,
  collection: params.collection,
  timestamp: new Date().toISOString(),
  ...differences
});
/**
 * DataSchema — Type-safe command executor
 *
 * Usage:
 *   import { DataSchema } from '...shared/DataSchemaTypes';
 *   const result = await DataSchema.execute({ ... });
 */
export const DataSchema = {
  execute(params: CommandInput<DataSchemaParams>): Promise<DataSchemaResult> {
    return Commands.execute<DataSchemaParams, DataSchemaResult>('data/schema', params as Partial<DataSchemaParams>);
  },
  commandName: 'data/schema' as const,
} as const;
