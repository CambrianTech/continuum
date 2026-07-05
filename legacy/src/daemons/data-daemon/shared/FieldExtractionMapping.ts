/**
 * Field Extraction Mapping - Adapter-agnostic field extraction configuration
 *
 * Defines which fields from entities should be extracted for optimized storage/querying
 * Used by all storage adapters (SQLite, Memory, File, etc.)
 */

export type FieldType = 'text' | 'integer' | 'real' | 'boolean' | 'datetime' | 'json';

export interface ExtractedField {
  name: string;
  type: FieldType;
  nullable: boolean;
  indexed?: boolean;
  converter?: {
    toStorage: (value: any) => any;
    fromStorage: (value: any) => any;
  };
}

export interface FieldExtractionMapping {
  collection: string;
  extractedFields: ExtractedField[];
  keepJsonBlob: boolean; // For backward compatibility during migration
}