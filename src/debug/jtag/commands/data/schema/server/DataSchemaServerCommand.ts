/**
 * Data Schema Server Command - Generic Entity Schema Introspection
 *
 * Architecture-compliant server command that works generically with ANY entity type
 * Uses only BaseEntity patterns and the entity registry system
 * NEVER references specific entity types per ARCHITECTURE-RULES.md
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataSchemaParams, DataSchemaResult, EntitySchema, SchemaField, EntityExamples, EntitySQL, ValidationResult } from '../shared/DataSchemaTypes';
import { createDataSchemaResultFromParams } from '../shared/DataSchemaTypes';
import { getFieldMetadata, hasFieldMetadata, type FieldMetadata } from '../../../../system/data/decorators/FieldDecorators';
import { BaseEntity } from '../../../../system/data/entities/BaseEntity';

// Use the existing entity registry system - works with ANY registered entity type
import { getRegisteredEntity } from '../../../../daemons/data-daemon/server/SqliteStorageAdapter';

export class DataSchemaServerCommand extends CommandBase<DataSchemaParams, DataSchemaResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-schema', context, subpath, commander);
  }

  async execute(params: DataSchemaParams): Promise<DataSchemaResult> {
    try {
      console.log(`🔍 DataSchema: Getting schema for collection "${params.collection}"`);

      let schema: EntitySchema;
      let validation: ValidationResult | undefined;

      // Get entity class from registry - works with ANY registered entity type generically
      const EntityClass = getRegisteredEntity(params.collection);
      if (EntityClass) {
        // Create instance to get collection name generically (per architecture rules)
        const entityInstance = new EntityClass() as BaseEntity;
        const collectionName = entityInstance.collection;

        console.log(`📋 DataSchema: Found registered entity for "${params.collection}" → ${EntityClass.name}`);

        // Try decorator approach first - works generically with any entity
        if (hasFieldMetadata(EntityClass)) {
          console.log(`✅ DataSchema: Using decorator metadata for ${collectionName}`);
          const fieldMetadata = getFieldMetadata(EntityClass);
          schema = this.buildEntitySchema(collectionName, EntityClass.name, fieldMetadata, params.examples, params.sql);
        } else {
          // Generic BaseEntity fallback - works with any entity extending BaseEntity
          console.log(`🔧 DataSchema: Using BaseEntity patterns for ${collectionName}`);
          schema = this.createGenericBaseEntitySchema(collectionName, EntityClass.name, params.examples, params.sql);
        }

        // Validate data generically if provided - works with any entity type
        if (params.validateData) {
          validation = await this.validateDataGenerically(params.validateData, entityInstance);
        }
      } else {
        // No entity registered - fall back to data inference (architecture compliant)
        console.log(`⚠️ DataSchema: No entity registered for "${params.collection}", using data inference`);
        schema = await this.inferSchemaFromData(params.collection, params.examples, params.sql);

        if (params.validateData) {
          validation = {
            valid: false,
            error: `Cannot validate data: no entity class registered for collection "${params.collection}"`
          };
        }
      }

      console.log(`✅ DataSchema: Schema ready for "${params.collection}" with ${schema.fields.length} fields`);

      return createDataSchemaResultFromParams(params, {
        success: true,
        schema,
        validation
      });
    } catch (error) {
      console.error(`❌ DataSchema: Error for collection "${params.collection}":`, error);
      return createDataSchemaResultFromParams(params, {
        success: false,
        error: error instanceof Error ? error.message : 'Schema generation failed'
      });
    }
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

  /**
   * Fallback: Infer schema from existing data in the collection
   * Analyzes actual data structure to create schema when entity registry isn't available
   */
  private async inferSchemaFromData(
    collection: string,
    includeExamples?: boolean,
    includeSql?: boolean
  ): Promise<EntitySchema> {
    console.log(`🔍 DataSchema: Inferring schema from data for ${collection}`);

    try {
      // Try to get sample data from the collection
      const sampleData = await this.getSampleData(collection);

      if (!sampleData || sampleData.length === 0) {
        // No data available, create minimal BaseEntity schema
        return this.createBaseEntitySchema(collection, includeExamples, includeSql);
      }

      // Analyze the sample data to infer field types
      const inferredFields = this.analyzeDataStructure(sampleData);

      const schema: EntitySchema = {
        collection,
        entityClass: 'InferredEntity',
        fields: inferredFields,
        primaryKey: 'id',
        indexes: ['id'],
        foreignKeys: [],
        requiredFields: ['id']
      };

      // Add examples if requested
      if (includeExamples) {
        schema.examples = this.generateInferredExamples(inferredFields, collection, sampleData);
      }

      // Add SQL if requested
      if (includeSql) {
        schema.sql = this.generateEntitySQL(collection, inferredFields, ['id'], []);
      }

      return schema;
    } catch (error) {
      console.error(`❌ DataSchema: Error inferring schema for ${collection}:`, error);
      // Fallback to BaseEntity schema
      return this.createBaseEntitySchema(collection, includeExamples, includeSql);
    }
  }

  /**
   * Get sample data from a collection for schema inference
   */
  private async getSampleData(collection: string): Promise<Record<string, any>[]> {
    try {
      // Create a mock data list params for the internal command call
      const listParams = {
        context: this.context,
        sessionId: '00000000-0000-0000-0000-000000000000' as any,
        collection,
        limit: 5 // Get 5 sample records for analysis
      };

      // For now, return empty array - the fallback schema system will handle it
      // In a full implementation, we'd need access to the data daemon directly
      console.log(`🔍 DataSchema: Sample data retrieval not implemented, using BaseEntity fallback`);
      return [];
    } catch (error) {
      console.error(`❌ DataSchema: Error getting sample data for ${collection}:`, error);
      return [];
    }
  }

  /**
   * Analyze data structure to infer field types
   */
  private analyzeDataStructure(sampleData: Record<string, any>[]): SchemaField[] {
    const fieldTypes = new Map<string, Set<string>>();
    const fieldExamples = new Map<string, any[]>();

    // Analyze all records to find field patterns
    for (const record of sampleData) {
      for (const [fieldName, value] of Object.entries(record)) {
        if (!fieldTypes.has(fieldName)) {
          fieldTypes.set(fieldName, new Set());
          fieldExamples.set(fieldName, []);
        }

        const inferredType = this.inferFieldType(value);
        fieldTypes.get(fieldName)!.add(inferredType);
        fieldExamples.get(fieldName)!.push(value);
      }
    }

    // Convert to SchemaField array
    const fields: SchemaField[] = [];
    for (const [fieldName, types] of fieldTypes.entries()) {
      const examples = fieldExamples.get(fieldName) || [];

      // Determine the most appropriate type
      let fieldType: SchemaField['fieldType'];
      if (fieldName === 'id') {
        fieldType = 'primary';
      } else if (types.has('date')) {
        fieldType = 'date';
      } else if (types.has('number')) {
        fieldType = 'number';
      } else if (types.has('boolean')) {
        fieldType = 'boolean';
      } else if (types.has('json')) {
        fieldType = 'json';
      } else {
        fieldType = 'text';
      }

      const field: SchemaField = {
        fieldName,
        fieldType,
        required: fieldName === 'id', // Only ID is definitely required
        nullable: fieldName !== 'id'
      };

      fields.push(field);
    }

    return fields;
  }

  /**
   * Infer the type of a field value
   */
  private inferFieldType(value: any): string {
    if (value === null || value === undefined) {
      return 'text'; // Default for null values
    }

    if (typeof value === 'number') {
      return 'number';
    }

    if (typeof value === 'boolean') {
      return 'boolean';
    }

    if (typeof value === 'string') {
      // Check if it looks like a date
      if (value.match(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/)) {
        return 'date';
      }
      return 'text';
    }

    if (typeof value === 'object') {
      return 'json';
    }

    return 'text';
  }

  /**
   * Create minimal BaseEntity schema when no data is available
   */
  private createBaseEntitySchema(
    collection: string,
    includeExamples?: boolean,
    includeSql?: boolean
  ): EntitySchema {
    const baseFields: SchemaField[] = [
      { fieldName: 'id', fieldType: 'primary', required: true, nullable: false },
      { fieldName: 'createdAt', fieldType: 'date', required: true, nullable: false },
      { fieldName: 'updatedAt', fieldType: 'date', required: true, nullable: false },
      { fieldName: 'version', fieldType: 'number', required: true, nullable: false }
    ];

    const schema: EntitySchema = {
      collection,
      entityClass: 'BaseEntity',
      fields: baseFields,
      primaryKey: 'id',
      indexes: ['id', 'createdAt', 'updatedAt'],
      foreignKeys: [],
      requiredFields: ['id', 'createdAt', 'updatedAt', 'version']
    };

    if (includeExamples) {
      schema.examples = this.generateEntityExamples(baseFields, collection);
    }

    if (includeSql) {
      schema.sql = this.generateEntitySQL(collection, baseFields, schema.indexes, []);
    }

    return schema;
  }

  /**
   * Generate examples based on inferred schema and actual data
   */
  private generateInferredExamples(
    fields: SchemaField[],
    collection: string,
    sampleData: Record<string, any>[]
  ): EntityExamples {
    const actualExample = sampleData[0] || {};

    const minimal: Record<string, any> = {};
    const complete: Record<string, any> = {};

    for (const field of fields) {
      const actualValue = actualExample[field.fieldName];
      const exampleValue = actualValue !== undefined ? actualValue : this.getExampleValue(field);

      complete[field.fieldName] = exampleValue;
      if (field.required) {
        minimal[field.fieldName] = exampleValue;
      }
    }

    return {
      minimal,
      complete,
      description: `Schema inferred from existing ${collection} data. Based on ${sampleData.length} sample records.`
    };
  }

  /**
   * Validate JSON data against a schema
   */
  private async validateJsonData(jsonData: Record<string, unknown>, schema: EntitySchema): Promise<ValidationResult> {
    const errorMessages: string[] = [];

    try {
      // Validate against schema fields (jsonData is already parsed)
      const validatedData: Record<string, unknown> = {};

      // Check required fields
      for (const requiredField of schema.requiredFields) {
        if (!(requiredField in jsonData)) {
          // Allow auto-generation for BaseEntity fields
          if (['id', 'createdAt', 'updatedAt', 'version'].includes(requiredField)) {
            continue;
          }
          errorMessages.push(`Missing required field: ${requiredField}`);
        }
      }

      // Validate field types and values
      for (const [fieldName, value] of Object.entries(jsonData)) {
        const field = schema.fields.find(f => f.fieldName === fieldName);

        if (!field) {
          validatedData[fieldName] = value;
          continue;
        }

        const fieldValidation = this.validateFieldValue(fieldName, value, field);
        if (fieldValidation.valid) {
          validatedData[fieldName] = fieldValidation.value;
        } else {
          errorMessages.push(...fieldValidation.errors);
        }
      }

      // Add auto-generated BaseEntity fields if not present
      if (!jsonData.id) validatedData.id = '{{auto-generated}}';
      if (!jsonData.createdAt) validatedData.createdAt = new Date().toISOString();
      if (!jsonData.updatedAt) validatedData.updatedAt = new Date().toISOString();
      if (!jsonData.version) validatedData.version = 0;

      if (errorMessages.length === 0) {
        return {
          valid: true,
          validatedEntity: validatedData
        };
      } else {
        return {
          valid: false,
          error: errorMessages.join('; ')
        };
      }
    } catch (error) {
      return {
        valid: false,
        error: `Validation error: ${error instanceof Error ? error.message : 'Unknown error'}`
      };
    }
  }

  /**
   * Validate a single field value against its schema
   */
  private validateFieldValue(
    fieldName: string,
    value: any,
    field: SchemaField
  ): { valid: boolean; value?: any; errors: string[] } {
    const errors: string[] = [];

    // Check null/undefined
    if (value === null || value === undefined) {
      if (!field.nullable) {
        errors.push(`Field "${fieldName}" cannot be null`);
        return { valid: false, errors };
      }
      return { valid: true, value, errors: [] };
    }

    // Type-specific validation
    switch (field.fieldType) {
      case 'text':
        if (typeof value !== 'string') {
          errors.push(`Field "${fieldName}" must be a string`);
          return { valid: false, errors };
        }
        if (field.maxLength && value.length > field.maxLength) {
          errors.push(`Field "${fieldName}" exceeds maximum length of ${field.maxLength}`);
          return { valid: false, errors };
        }
        break;

      case 'number':
        if (typeof value !== 'number' && !Number.isFinite(Number(value))) {
          errors.push(`Field "${fieldName}" must be a number`);
          return { valid: false, errors };
        }
        return { valid: true, value: Number(value), errors: [] };

      case 'boolean':
        if (typeof value !== 'boolean') {
          errors.push(`Field "${fieldName}" must be a boolean`);
          return { valid: false, errors };
        }
        break;

      case 'date':
        if (typeof value === 'string') {
          const date = new Date(value);
          if (isNaN(date.getTime())) {
            errors.push(`Field "${fieldName}" must be a valid date string`);
            return { valid: false, errors };
          }
          return { valid: true, value: date.toISOString(), errors: [] };
        }
        errors.push(`Field "${fieldName}" must be a date string`);
        return { valid: false, errors };

      case 'json':
        // JSON fields accept any value, but objects should be valid
        if (typeof value === 'object') {
          try {
            JSON.stringify(value);
          } catch {
            errors.push(`Field "${fieldName}" contains invalid JSON object`);
            return { valid: false, errors };
          }
        }
        break;

      case 'primary':
        if (typeof value !== 'string') {
          errors.push(`Primary key "${fieldName}" must be a string`);
          return { valid: false, errors };
        }
        break;
    }

    return { valid: true, value, errors: [] };
  }

  /**
   * Create generic BaseEntity schema - works with ANY entity extending BaseEntity
   * Architecture compliant: uses entity.collection, no hardcoded entity types
   */
  private createGenericBaseEntitySchema(
    collectionName: string,
    entityClassName: string,
    includeExamples?: boolean,
    includeSql?: boolean
  ): EntitySchema {
    console.log(`🔧 DataSchema: Creating BaseEntity schema for ${collectionName}`);

    const baseFields: SchemaField[] = [
      { fieldName: 'id', fieldType: 'primary', required: true, nullable: false, unique: true },
      { fieldName: 'createdAt', fieldType: 'date', required: true, nullable: false, index: true },
      { fieldName: 'updatedAt', fieldType: 'date', required: true, nullable: false, index: true },
      { fieldName: 'version', fieldType: 'number', required: true, nullable: false }
    ];

    const schema: EntitySchema = {
      collection: collectionName,
      entityClass: entityClassName,
      fields: baseFields,
      primaryKey: 'id',
      indexes: ['createdAt', 'updatedAt'],
      foreignKeys: [],
      requiredFields: ['id', 'createdAt', 'updatedAt', 'version']
    };

    if (includeExamples) {
      schema.examples = this.generateEntityExamples(baseFields, collectionName);
    }

    if (includeSql) {
      schema.sql = this.generateEntitySQL(collectionName, baseFields, schema.indexes, []);
    }

    return schema;
  }

  /**
   * Validate data generically using BaseEntity patterns - works with ANY entity type
   * Architecture compliant: uses entity.validate(), no entity-specific logic
   */
  private async validateDataGenerically(
    data: Record<string, unknown>,
    entityInstance: BaseEntity
  ): Promise<ValidationResult> {
    try {
      console.log(`🔧 DataSchema: Validating data for ${entityInstance.collection} using ${entityInstance.constructor.name}`);

      // Populate the entity instance with provided data
      Object.assign(entityInstance, data);

      // Use the entity's own validate method - works generically with any entity
      const validationResult = entityInstance.validate();

      const result: ValidationResult = {
        valid: validationResult.success,
        error: validationResult.error,
        validatedEntity: validationResult.success ? entityInstance : undefined
      };

      console.log(`${result.valid ? '✅' : '❌'} DataSchema: Generic validation ${result.valid ? 'passed' : 'failed'}${result.error ? ': ' + result.error : ''}`);
      return result;

    } catch (error) {
      const result: ValidationResult = {
        valid: false,
        error: `Generic validation failed: ${error instanceof Error ? error.message : String(error)}`
      };
      console.log(`❌ DataSchema: Generic validation error: ${result.error}`);
      return result;
    }
  }
}