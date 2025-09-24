/**
 * Data Schema Command - Entity Schema Introspection
 *
 * Provides runtime schema information from entity decorators
 * Allows callers to understand entity structure, constraints, and validation rules
 */

import type { JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { FieldMetadata } from '../../../../system/data/decorators/FieldDecorators';

/**
 * Request parameters for data/schema command
 */
export interface DataSchemaParams extends JTAGPayload {
  readonly collection: string; // Entity collection name to get schema for
}

/**
 * Enhanced field information including decorator metadata
 */
export interface SchemaField {
  fieldName: string;
  fieldType: 'primary' | 'foreign_key' | 'date' | 'enum' | 'text' | 'json' | 'number' | 'boolean';
  required: boolean; // Derived from !nullable
  nullable: boolean;
  index?: boolean;
  unique?: boolean;
  maxLength?: number;
  default?: any;
  references?: string; // For foreign keys: 'User.userId'
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