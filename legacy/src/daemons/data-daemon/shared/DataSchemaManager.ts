/**
 * DataSchemaManager — Entity schema extraction, validation, and provisioning
 *
 * Extracted from DataDaemon. Knows about entity decorators and registry,
 * translates them into generic CollectionSchema for storage adapters.
 *
 * ARCHITECTURE:
 * - Schema manager knows entities and their decorators
 * - Extracts schema and passes it to adapters in a generic format
 * - Adapters don't need to know about ENTITY_REGISTRY or decorators
 */

import { BaseEntity } from '../../../system/data/entities/BaseEntity';
import type {
  DataStorageAdapter,
  CollectionSchema,
  SchemaField,
  SchemaFieldType,
  SchemaIndex,
} from './DataStorageAdapter';

import { getRegisteredEntity, ENTITY_REGISTRY } from '../server/EntityRegistry';

import {
  getFieldMetadata,
  hasFieldMetadata,
  getCompositeIndexes,
  type FieldType,
} from '../../../system/data/decorators/FieldDecorators';

/**
 * Entity Constructor Type with BaseEntity static methods
 */
type EntityConstructor = (new (...args: unknown[]) => BaseEntity) & typeof BaseEntity;

export interface SchemaValidationResult {
  success: boolean;
  errors?: string[];
}

export class DataSchemaManager {
  private readonly ensuredSchemas: Set<string> = new Set();

  /**
   * Map decorator FieldType to generic SchemaFieldType
   */
  mapFieldTypeToSchemaType(fieldType: FieldType): SchemaFieldType {
    switch (fieldType) {
      case 'primary':
      case 'foreign_key':
        return 'uuid';
      case 'date':
        return 'date';
      case 'text':
      case 'enum':
        return 'string';
      case 'json':
        return 'json';
      case 'number':
        return 'number';
      case 'boolean':
        return 'boolean';
      default:
        return 'string';
    }
  }

  /**
   * Extract CollectionSchema from entity decorators.
   *
   * Returns undefined if no entity is registered or has no field metadata,
   * allowing the adapter to use fallback behavior for unregistered collections.
   */
  extractCollectionSchema(collection: string): CollectionSchema | undefined {
    const entityClass = ENTITY_REGISTRY.get(collection) as EntityConstructor | undefined;
    if (!entityClass || !hasFieldMetadata(entityClass)) {
      return undefined;
    }

    const fieldMetadata = getFieldMetadata(entityClass);
    const fields: SchemaField[] = [];

    for (const [fieldName, metadata] of fieldMetadata) {
      fields.push({
        name: fieldName,
        type: this.mapFieldTypeToSchemaType(metadata.fieldType),
        indexed: metadata.options?.index,
        unique: metadata.options?.unique,
        nullable: metadata.options?.nullable,
        maxLength: metadata.options?.maxLength,
      });
    }

    const compositeIndexes = getCompositeIndexes(entityClass);
    const indexes: SchemaIndex[] = compositeIndexes.map(idx => ({
      name: idx.name,
      fields: idx.fields,
      unique: idx.unique,
    }));

    return {
      collection,
      fields,
      indexes: indexes.length > 0 ? indexes : undefined,
    };
  }

  /**
   * Ensure collection schema exists via adapter.
   *
   * - Extracts schema from entity decorators (knows entities)
   * - Caches which collections are ensured (avoids repeated calls)
   * - Passes schema to adapter.ensureSchema() for implementation
   */
  async ensureSchema(collection: string, adapter: DataStorageAdapter): Promise<void> {
    if (this.ensuredSchemas.has(collection)) {
      return;
    }

    const schema = this.extractCollectionSchema(collection);
    const result = await adapter.ensureSchema(collection, schema);
    if (!result.success) {
      throw new Error(`Failed to ensure schema for ${collection}: ${result.error}`);
    }

    this.ensuredSchemas.add(collection);
  }

  /**
   * Validate entity data against its registered entity class.
   *
   * If no entity is registered for the collection, validation is SKIPPED
   * (allows custom collections that manage their own schema).
   */
  validateEntity(collection: string, data: Record<string, unknown>): SchemaValidationResult {
    const EntityClass = getRegisteredEntity(collection) as EntityConstructor;
    if (!EntityClass) {
      console.log(`⚠️ DataSchemaManager: No entity registered for "${collection}" - skipping validation (custom collection)`);
      return { success: true };
    }

    const entityResult = EntityClass.create(data);
    if (!entityResult.success || !entityResult.entity) {
      console.error(`❌ DataSchemaManager: Entity creation failed for "${collection}":`, entityResult.error);
      return { success: false, errors: [entityResult.error || 'Entity creation failed'] };
    }

    const validationResult = entityResult.entity.validate();
    if (!validationResult.success) {
      console.error(`❌ DataSchemaManager: Entity validation failed for "${collection}":`, validationResult.error);
      return { success: false, errors: [validationResult.error || 'Validation failed'] };
    }

    return { success: true };
  }

  /**
   * Ensure schema on an arbitrary adapter (for per-persona dbHandle adapters
   * that bypass DataDaemon's default adapter).
   */
  async ensureAdapterSchema(adapter: DataStorageAdapter, collection: string): Promise<void> {
    const schema = this.extractCollectionSchema(collection);
    if (schema) {
      await adapter.ensureSchema(collection, schema);
    }
  }
}
