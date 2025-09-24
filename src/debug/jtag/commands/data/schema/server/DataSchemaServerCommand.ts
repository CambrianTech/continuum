/**
 * Data Schema Server Command - Entity Schema Introspection
 *
 * Server-side command that uses the entity registry to provide schema information
 * from decorator metadata for any registered entity collection
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataSchemaParams, DataSchemaResult, EntitySchema, SchemaField } from '../shared/DataSchemaTypes';
import { createDataSchemaResultFromParams } from '../shared/DataSchemaTypes';
import { getFieldMetadata, hasFieldMetadata, type FieldMetadata } from '../../../../system/data/decorators/FieldDecorators';

// Import entity registry to access registered entities
import { getRegisteredEntity } from '../../../../daemons/data-daemon/server/SqliteStorageAdapter';

export class DataSchemaServerCommand extends CommandBase<DataSchemaParams, DataSchemaResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-schema', context, subpath, commander);
  }

  async execute(params: DataSchemaParams): Promise<DataSchemaResult> {
    console.log(`🔍 DataSchema: Getting schema for collection "${params.collection}"`);

    // Get entity class from registry
    const entityClass = getRegisteredEntity(params.collection);
    if (!entityClass) {
      throw new Error(`No entity registered for collection "${params.collection}"`);
    }

    // Check if entity has field metadata from decorators
    if (!hasFieldMetadata(entityClass)) {
      throw new Error(`Entity class for "${params.collection}" has no field decorators`);
    }

    // Extract field metadata from decorators
    const fieldMetadata = getFieldMetadata(entityClass);
    const schema = this.buildEntitySchema(params.collection, entityClass.name, fieldMetadata);

    console.log(`✅ DataSchema: Schema extracted for "${params.collection}" with ${schema.fields.length} fields`);

    return createDataSchemaResultFromParams(params, {
      success: true,
      schema
    });
  }

  /**
   * Convert field metadata from decorators into structured schema information
   */
  private buildEntitySchema(collection: string, className: string, fieldMetadata: Map<string, FieldMetadata>): EntitySchema {
    const fields: SchemaField[] = [];
    const indexes: string[] = [];
    const foreignKeys: Array<{ field: string; references: string }> = [];
    const requiredFields: string[] = [];
    let primaryKey: string | undefined;

    // Process each field from decorator metadata
    for (const [fieldName, metadata] of fieldMetadata.entries()) {
      const schemaField: SchemaField = {
        fieldName,
        fieldType: metadata.fieldType,
        required: !metadata.options?.nullable,
        nullable: metadata.options?.nullable ?? false,
        index: metadata.options?.index,
        unique: metadata.options?.unique,
        maxLength: metadata.options?.maxLength,
        default: metadata.options?.default,
        references: metadata.options?.references
      };

      fields.push(schemaField);

      // Track primary key
      if (metadata.fieldType === 'primary') {
        primaryKey = fieldName;
      }

      // Track indexed fields
      if (metadata.options?.index) {
        indexes.push(fieldName);
      }

      // Track foreign keys
      if (metadata.fieldType === 'foreign_key' && metadata.options?.references) {
        foreignKeys.push({
          field: fieldName,
          references: metadata.options.references
        });
      }

      // Track required fields
      if (!metadata.options?.nullable) {
        requiredFields.push(fieldName);
      }
    }

    return {
      collection,
      entityClass: className,
      fields,
      primaryKey,
      indexes,
      foreignKeys,
      requiredFields
    };
  }
}