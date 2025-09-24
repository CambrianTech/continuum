/**
 * Data Schema Server Command - Entity Schema Introspection
 *
 * Server-side command that uses the entity registry to provide schema information
 * from decorator metadata for any registered entity collection
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataSchemaParams, DataSchemaResult, EntitySchema, SchemaField, EntityExamples, EntitySQL } from '../shared/DataSchemaTypes';
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
    const schema = this.buildEntitySchema(params.collection, entityClass.name, fieldMetadata, params.examples, params.sql);

    console.log(`✅ DataSchema: Schema extracted for "${params.collection}" with ${schema.fields.length} fields`);

    return createDataSchemaResultFromParams(params, {
      success: true,
      schema
    });
  }

  /**
   * Convert field metadata from decorators into structured schema information
   */
  private buildEntitySchema(
    collection: string,
    className: string,
    fieldMetadata: Map<string, FieldMetadata>,
    includeExamples?: boolean,
    includeSql?: boolean
  ): EntitySchema {
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

    const schema: EntitySchema = {
      collection,
      entityClass: className,
      fields,
      primaryKey,
      indexes,
      foreignKeys,
      requiredFields
    };

    // Add examples if requested
    if (includeExamples) {
      schema.examples = this.generateEntityExamples(fields, collection);
    }

    // Add SQL statements if requested
    if (includeSql) {
      schema.sql = this.generateEntitySQL(collection, fields, indexes, foreignKeys);
    }

    return schema;
  }

  /**
   * Generate example JSON objects for entity creation
   */
  private generateEntityExamples(fields: SchemaField[], collection: string): EntityExamples {
    const minimal: Record<string, any> = {};
    const complete: Record<string, any> = {};

    // Process each field to create examples
    for (const field of fields) {
      const exampleValue = this.getExampleValue(field);

      // Add to complete example
      complete[field.fieldName] = exampleValue;

      // Add to minimal example only if required
      if (field.required) {
        minimal[field.fieldName] = exampleValue;
      }
    }

    return {
      minimal,
      complete,
      description: `Minimal example includes only required fields (${Object.keys(minimal).length} fields). Complete example includes all fields with realistic sample values (${Object.keys(complete).length} fields).`
    };
  }

  /**
   * Generate realistic example value based on field type and constraints
   */
  private getExampleValue(field: SchemaField): any {
    switch (field.fieldType) {
      case 'primary':
        return `${field.fieldName}_${Date.now()}`;

      case 'foreign_key':
        const refTable = field.references?.split('.')[0] || 'other';
        return `${refTable}_id_example`;

      case 'date':
        return new Date().toISOString();

      case 'enum':
        return 'enum_value_example';

      case 'text':
        if (field.fieldName.toLowerCase().includes('email')) {
          return 'user@example.com';
        }
        if (field.fieldName.toLowerCase().includes('name')) {
          return `Example ${field.fieldName}`;
        }
        if (field.fieldName.toLowerCase().includes('description')) {
          return `Sample ${field.fieldName} for demonstration`;
        }
        const maxLen = field.maxLength || 50;
        return `Sample ${field.fieldName}`.substring(0, maxLen);

      case 'json':
        if (field.fieldName.toLowerCase().includes('profile')) {
          return {
            displayName: 'Example User',
            bio: 'Sample user profile',
            avatar: '👤',
            joinedAt: new Date().toISOString()
          };
        }
        if (field.fieldName.toLowerCase().includes('config') || field.fieldName.toLowerCase().includes('setting')) {
          return {
            enabled: true,
            value: 'default',
            updatedAt: new Date().toISOString()
          };
        }
        return { key: 'value', example: true };

      case 'number':
        if (field.fieldName.toLowerCase().includes('count')) {
          return 0;
        }
        return 42;

      case 'boolean':
        return true;

      default:
        return field.default || null;
    }
  }

  /**
   * Generate SQL statements for entity creation
   */
  private generateEntitySQL(
    collection: string,
    fields: SchemaField[],
    indexes: string[],
    foreignKeys: Array<{ field: string; references: string }>
  ): EntitySQL {
    const tableName = collection.toLowerCase();

    // Generate CREATE TABLE statement
    const columnDefinitions = fields.map(field => {
      const columnName = this.toSnakeCase(field.fieldName);
      let sqlType = this.getSQLType(field.fieldType);

      const constraints = [];
      if (!field.nullable) constraints.push('NOT NULL');
      if (field.unique) constraints.push('UNIQUE');
      if (field.fieldType === 'primary') constraints.push('PRIMARY KEY');
      if (field.default !== undefined) {
        const defaultValue = typeof field.default === 'string' ? `'${field.default}'` : field.default;
        constraints.push(`DEFAULT ${defaultValue}`);
      }

      return `  ${columnName} ${sqlType}${constraints.length ? ' ' + constraints.join(' ') : ''}`;
    });

    const createTable = `CREATE TABLE IF NOT EXISTS ${tableName} (\n${columnDefinitions.join(',\n')}\n);`;

    // Generate INDEX statements
    const indexStatements = indexes.map(fieldName => {
      const columnName = this.toSnakeCase(fieldName);
      return `CREATE INDEX IF NOT EXISTS idx_${tableName}_${columnName} ON ${tableName}(${columnName});`;
    });

    // Generate FOREIGN KEY statements
    const foreignKeyStatements = foreignKeys.map(fk => {
      const columnName = this.toSnakeCase(fk.field);
      const [refTable, refColumn] = fk.references.split('.');
      const refTableName = refTable.toLowerCase();
      const refColumnName = this.toSnakeCase(refColumn);
      return `ALTER TABLE ${tableName} ADD FOREIGN KEY (${columnName}) REFERENCES ${refTableName}(${refColumnName});`;
    });

    return {
      createTable,
      indexes: indexStatements,
      foreignKeys: foreignKeyStatements
    };
  }

  /**
   * Convert camelCase to snake_case for SQL column names
   */
  private toSnakeCase(str: string): string {
    return str.replace(/[A-Z]/g, letter => `_${letter.toLowerCase()}`).replace(/^_/, '');
  }

  /**
   * Map field types to SQL types
   */
  private getSQLType(fieldType: string): string {
    switch (fieldType) {
      case 'primary':
      case 'foreign_key':
      case 'text':
        return 'TEXT';
      case 'date':
        return 'DATETIME';
      case 'enum':
        return 'TEXT';
      case 'json':
        return 'TEXT';
      case 'number':
        return 'INTEGER';
      case 'boolean':
        return 'BOOLEAN';
      default:
        return 'TEXT';
    }
  }
}