/**
 * Entity Metadata Extractor - Decorator-based Schema Extraction
 *
 * Replaces manual field mappings with decorator metadata extraction
 * Implements the user's vision: decorators define "what the field is, what the table is"
 */

import {
  getFieldMetadata,
  hasFieldMetadata,
  type FieldMetadata,
  type FieldType
} from './FieldDecorators';
import { COLLECTIONS } from '../core/FieldMapping';
import { ChatMessageEntity } from '../entities/ChatMessageEntity';
import { UserEntity } from '../entities/UserEntity';
import { RoomEntity } from '../entities/RoomEntity';
import { BaseEntity } from '../domains/CoreTypes';

/**
 * Field extraction configuration derived from decorators
 * Replaces the manual EntityFieldConfig system
 */
export interface DecoratorFieldMapping {
  collection: string;
  entityClass: Function;
  extractedFields: DecoratorExtractedField[];
  keepJsonBlob: boolean;
}

/**
 * Extracted field definition from decorators
 * Maps decorator FieldType to SQLite storage types
 */
export interface DecoratorExtractedField {
  fieldName: string;
  sqliteType: 'text' | 'integer' | 'real' | 'boolean' | 'datetime' | 'json';
  indexed: boolean;
  nullable: boolean;
  unique: boolean;
  converter?: {
    toStorage: (value: any) => any;
    fromStorage: (value: any) => any;
  };
}

/**
 * Map decorator FieldType to SQLite storage type
 */
function mapFieldTypeToSQLite(fieldType: FieldType): DecoratorExtractedField['sqliteType'] {
  switch (fieldType) {
    case 'primary':
    case 'foreign_key':
    case 'text':
    case 'enum':
      return 'text';
    case 'number':
      return 'real';
    case 'boolean':
      return 'boolean';
    case 'date':
      return 'datetime';
    case 'json':
      return 'json';
    default:
      return 'text';
  }
}

/**
 * Create date converters for ISO string ↔ timestamp conversion
 */
function createDateConverter() {
  return {
    toStorage: (isoString: string | null) => isoString ? new Date(isoString).getTime() : null,
    fromStorage: (timestamp: number | null) => timestamp ? new Date(timestamp).toISOString() : null
  };
}

/**
 * Extract field configuration from entity decorators
 */
function extractFieldsFromDecorators(entityClass: Function): DecoratorExtractedField[] {
  const metadata = getFieldMetadata(entityClass);
  const fields: DecoratorExtractedField[] = [];

  for (const [fieldName, fieldMeta] of metadata.entries()) {
    const sqliteType = mapFieldTypeToSQLite(fieldMeta.fieldType);

    const field: DecoratorExtractedField = {
      fieldName,
      sqliteType,
      indexed: fieldMeta.options?.index ?? false,
      nullable: fieldMeta.options?.nullable ?? false,
      unique: fieldMeta.options?.unique ?? false
    };

    // Add date converters for datetime fields
    if (sqliteType === 'datetime') {
      field.converter = createDateConverter();
    }

    fields.push(field);
  }

  return fields;
}

// NOTE: This registry is not needed for Option 2 elegant system
// Collection mapping is handled by the DataListServerCommand type extraction
// const ENTITY_REGISTRY = ...

/**
 * Generate field mappings from decorator metadata for a specific entity class
 * This replaces the manual ENTITY_FIELD_MAPPINGS and hardcoded registry
 */
export function generateDecoratorFieldMapping(collection: string, entityClass: Function): DecoratorFieldMapping | null {
  if (!hasFieldMetadata(entityClass)) {
    console.warn(`⚠️ Entity ${entityClass.name} has no field metadata - skipping`);
    return null;
  }

  const extractedFields = extractFieldsFromDecorators(entityClass);

  console.log(`✅ Generated field mapping for ${collection} from ${entityClass.name} decorators`);
  console.log(`   Fields: ${extractedFields.map(f => f.fieldName).join(', ')}`);

  return {
    collection,
    entityClass,
    extractedFields,
    keepJsonBlob: true // For backward compatibility during migration
  };
}

/**
 * Get decorator-based field mapping for a collection and entity class
 * This replaces getEntityFieldMapping from EntityFieldConfig
 */
export function getDecoratorFieldMapping(collection: string, entityClass: Function): DecoratorFieldMapping | undefined {
  if (!entityClass || !hasFieldMetadata(entityClass)) {
    return undefined;
  }

  const extractedFields = extractFieldsFromDecorators(entityClass);

  return {
    collection,
    entityClass,
    extractedFields,
    keepJsonBlob: true
  };
}

/**
 * Check if entity class has decorator-based field extraction
 * This replaces hasFieldExtraction from EntityFieldConfig
 */
export function hasDecoratorFieldExtraction(entityClass: Function): boolean {
  return hasFieldMetadata(entityClass);
}

/**
 * Note: getEntityClass function removed - entity classes should be passed directly
 * from the type system, not looked up from collections
 */