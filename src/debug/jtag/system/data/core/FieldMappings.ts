/**
 * Field Mappings - Centralized type-safe field definitions for storage abstraction
 *
 * Single source of truth for mapping generic field names to backend-specific columns.
 * Used by all storage adapters to maintain backend independence.
 */

// Generic field names used throughout the application
export type GenericFieldName =
  | 'createdAt'
  | 'updatedAt'
  | 'id'
  | 'userId'
  | 'roomId'
  | 'content'
  | 'messageType'
  | 'metadata';

// SQLite-specific column names
export type SqliteColumnName =
  | 'created_at'
  | 'updated_at'
  | 'id'
  | 'user_id'
  | 'room_id'
  | 'content'
  | 'message_type'
  | 'metadata';

// Type-safe field mapping configuration
export interface FieldMappingConfig {
  readonly generic: GenericFieldName;
  readonly sqlite: SqliteColumnName;
  readonly jsonPath?: string; // For JSON column extraction
}

// Centralized field definitions - single source of truth
export const fieldMappings: readonly FieldMappingConfig[] = [
  { generic: 'createdAt', sqlite: 'created_at' },
  { generic: 'updatedAt', sqlite: 'updated_at' },
  { generic: 'id', sqlite: 'id' },
  { generic: 'userId', sqlite: 'user_id', jsonPath: '$.userId' },
  { generic: 'roomId', sqlite: 'room_id', jsonPath: '$.roomId' },
  { generic: 'content', sqlite: 'content', jsonPath: '$.content' },
  { generic: 'messageType', sqlite: 'message_type', jsonPath: '$.messageType' },
  { generic: 'metadata', sqlite: 'metadata', jsonPath: '$.metadata' }
] as const;

// Create lookup maps for efficient field resolution
export const genericToSqliteMap = new Map<GenericFieldName, SqliteColumnName>(
  fieldMappings.map(config => [config.generic, config.sqlite])
);

export const sqliteToGenericMap = new Map<SqliteColumnName, GenericFieldName>(
  fieldMappings.map(config => [config.sqlite, config.generic])
);

export const jsonPathMap = new Map<GenericFieldName, string>(
  fieldMappings
    .filter(config => config.jsonPath)
    .map(config => [config.generic, config.jsonPath!])
);

// Type-safe field mapping functions
export const mapGenericToSqlite = (field: GenericFieldName): SqliteColumnName => {
  const mapped = genericToSqliteMap.get(field);
  if (!mapped) {
    throw new Error(`Unknown generic field: ${field}`);
  }
  return mapped;
};

export const mapSqliteToGeneric = (column: SqliteColumnName): GenericFieldName => {
  const mapped = sqliteToGenericMap.get(column);
  if (!mapped) {
    throw new Error(`Unknown SQLite column: ${column}`);
  }
  return mapped;
};

export const getJsonPath = (field: GenericFieldName): string => {
  const jsonPath = jsonPathMap.get(field);
  if (jsonPath) {
    return jsonPath;
  }
  // Default JSON extraction for unknown fields
  return `$.${field}`;
};

// Helper function to resolve field to appropriate SQLite expression
export const resolveSqliteField = (field: GenericFieldName): string => {
  // Check if it's a direct column mapping first
  const directColumn = genericToSqliteMap.get(field);
  if (directColumn && ['created_at', 'updated_at', 'id'].includes(directColumn)) {
    return directColumn;
  }

  // Use JSON extraction for data column fields
  return `JSON_EXTRACT(data, '${getJsonPath(field)}')`;
};