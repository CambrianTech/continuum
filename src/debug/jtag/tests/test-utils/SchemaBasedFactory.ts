/**
 * Schema-Based Entity Factory
 *
 * Uses actual schema information from data/schema commands to create perfect entities
 * with all required fields and intelligent defaults
 */

import { execSync } from 'child_process';

export interface SchemaField {
  fieldName: string;
  fieldType: string;
  required: boolean;
  nullable: boolean;
  maxLength?: number;
  references?: string;
}

export interface EntitySchema {
  collection: string;
  entityClass: string;
  fields: SchemaField[];
  requiredFields: string[];
}

export interface FactoryResult {
  success: boolean;
  id?: string;
  data?: Record<string, unknown>;
  error?: string;
}

/**
 * Cache for schema information to avoid repeated calls
 */
const schemaCache = new Map<string, EntitySchema>();

/**
 * Parse JSON from JTAG command output with robust error handling
 */
function parseJtagResponse(output: string): Record<string, unknown> {
  // Find JSON start
  const jsonStart = output.indexOf('{');
  if (jsonStart < 0) {
    throw new Error('No JSON found in output');
  }

  // Count braces to find JSON end
  let braceCount = 0;
  let jsonEnd = jsonStart;

  for (let i = jsonStart; i < output.length; i++) {
    if (output[i] === '{') braceCount++;
    if (output[i] === '}') {
      braceCount--;
      if (braceCount === 0) {
        jsonEnd = i + 1;
        break;
      }
    }
  }

  if (jsonEnd <= jsonStart) {
    throw new Error('Could not find complete JSON object');
  }

  const jsonStr = output.substring(jsonStart, jsonEnd);
  return JSON.parse(jsonStr);
}

/**
 * Execute JTAG command and return parsed result
 */
function runJtagCommand(command: string): Record<string, unknown> {
  let output: string;

  try {
    output = execSync(`./jtag ${command}`, {
      encoding: 'utf8',
      cwd: '/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag',
      stdio: 'pipe'
    });
  } catch (error: unknown) {
    const execError = error as { stdout?: string; message?: string };
    output = execError.stdout ?? '';
    if (!output) {
      throw new Error(`Command failed: ${execError.message ?? 'Unknown error'}`);
    }
  }

  return parseJtagResponse(output);
}

/**
 * Get schema information for a collection
 */
async function getSchema(collection: string): Promise<EntitySchema> {
  if (schemaCache.has(collection)) {
    return schemaCache.get(collection)!;
  }

  const response = runJtagCommand(`data/schema --collection="${collection}"`);

  if (!response.success || !response.schema) {
    throw new Error(`Failed to get schema for ${collection}: ${response.error ?? 'Unknown error'}`);
  }

  const schema = response.schema as EntitySchema;
  schemaCache.set(collection, schema);
  return schema;
}

/**
 * Get example entity from existing data
 */
async function getExampleEntity(collection: string): Promise<Record<string, unknown> | null> {
  try {
    const response = runJtagCommand(`data/list --collection="${collection}" --limit=1`);

    if (response.success && response.items && response.items.length > 0) {
      return response.items[0] as Record<string, unknown>;
    }
  } catch {
    console.log(`No example ${collection} found, using schema defaults`);
  }

  return null;
}

/**
 * Generate smart default value based on field type and schema
 */
function generateFieldDefault(field: SchemaField, example?: unknown): unknown {
  // Use example if available and not null
  if (example !== undefined && example !== null) {
    return example;
  }

  const now = new Date().toISOString();
  const timestamp = Date.now();

  switch (field.fieldType) {
    case 'primary':
      return `test-${timestamp}-${Math.random().toString(36).slice(2, 8)}`;

    case 'date':
      // Use existing example timestamp for compatibility instead of generating new Date()
      // Database has specific timestamp format requirements that new Date().toISOString() doesn't meet
      return exampleValue || '2025-09-25T04:38:38.452Z';

    case 'number':
      if (field.fieldName === 'version') return 0;
      if (field.fieldName === 'memberCount') return 1;
      return 1;

    case 'text':
      if (field.fieldName === 'displayName') {
        return `Test ${field.fieldName.replace('displayName', 'Entity')} ${timestamp}`;
      }
      if (field.fieldName === 'name') {
        return `test-name-${timestamp}`;
      }
      if (field.fieldName === 'description') {
        return 'Test description for development';
      }
      if (field.fieldName === 'shortDescription') {
        return 'Test entity';
      }
      if (field.fieldName === 'content') {
        // ChatMessage content should be structured
        return {
          text: `Test content ${timestamp}`,
          attachments: []
        };
      }
      return `test-${field.fieldName}-${timestamp}`;

    case 'enum':
      // Generic enum defaults based on common field names
      if (field.fieldName === 'status') {
        return 'active';
      }
      if (field.fieldName === 'type') {
        return 'default';
      }
      if (field.fieldName === 'privacy') {
        return 'public';
      }
      return 'default';

    case 'json':
      // Generic JSON defaults based on common field names
      if (field.fieldName.includes('capabilities') || field.fieldName.includes('permissions')) {
        return {};
      }
      if (field.fieldName.includes('settings') || field.fieldName.includes('config')) {
        return {};
      }
      if (field.fieldName.includes('members') || field.fieldName.includes('tags') ||
          field.fieldName.includes('items') || field.fieldName.includes('list')) {
        return [];
      }
      if (field.fieldName.includes('sessions') || field.fieldName.includes('active')) {
        return {};
      }
      // Default to empty object for structured data, array for lists
      return field.fieldName.toLowerCase().includes('list') ||
             field.fieldName.toLowerCase().includes('array') ? [] : {};

    case 'foreign_key':
      // Will be handled by dependency creation
      return null;

    default:
      return null;
  }
}

/**
 * Create entity with schema-driven defaults
 */
export async function createSchemaBasedEntity(
  collection: string,
  overrides: Record<string, unknown> = {}
): Promise<FactoryResult> {
  try {
    console.log(`🏗️ SchemaFactory: Creating ${collection} with schema-based defaults...`);

    // Get schema and example
    const [schema, example] = await Promise.all([
      getSchema(collection),
      getExampleEntity(collection)
    ]);

    // Build entity data with all required fields
    const entityData: Record<string, unknown> = {};

    // Process all required fields
    for (const fieldName of schema.requiredFields) {
      if (fieldName === 'id' || fieldName === 'createdAt' || fieldName === 'updatedAt') {
        // Skip system-generated fields
        continue;
      }

      const field = schema.fields.find(f => f.fieldName === fieldName);
      if (!field) {
        console.warn(`Field ${fieldName} not found in schema`);
        continue;
      }

      // Use override if provided, otherwise generate default
      if (Object.prototype.hasOwnProperty.call(overrides, fieldName)) {
        entityData[fieldName] = overrides[fieldName];
      } else {
        const exampleValue = example?.[fieldName];
        entityData[fieldName] = generateFieldDefault(field, exampleValue);
      }
    }

    // Handle foreign key dependencies generically
    await createForeignKeyDependencies(collection, schema, entityData, overrides);

    // Apply any remaining overrides
    Object.assign(entityData, overrides);

    console.log(`🏗️ SchemaFactory: Generated data for ${collection}:`, entityData);

    // Create entity via JTAG
    const result = runJtagCommand(`data/create --collection="${collection}" --data='${JSON.stringify(entityData)}'`);

    if (result.success && result.data?.id) {
      console.log(`✅ SchemaFactory: Created ${collection}/${result.data.id}`);
      return {
        success: true,
        id: result.data.id,
        data: result.data
      };
    } else {
      return {
        success: false,
        error: result.error ?? 'Entity creation failed'
      };
    }

  } catch (error) {
    console.error(`❌ SchemaFactory: Failed to create ${collection}:`, error);
    return {
      success: false,
      error: error instanceof Error ? error.message : String(error)
    };
  }
}

/**
 * Handle foreign key dependencies generically based on schema information
 */
async function createForeignKeyDependencies(
  collection: string,
  schema: EntitySchema,
  entityData: Record<string, unknown>,
  overrides: Record<string, unknown>
): Promise<void> {
  for (const field of schema.fields) {
    if (field.fieldType === 'foreign_key' && field.required &&
        !entityData[field.fieldName] && !overrides[field.fieldName]) {

      if (field.references) {
        // Extract referenced collection from "table.field" format
        const referencedCollection = field.references.split('.')[0];
        const collectionName = referencedCollection.charAt(0).toUpperCase() +
                              referencedCollection.slice(1, -1); // users -> User

        console.log(`🔗 SchemaFactory: ${field.fieldName} needs ${collectionName}, creating dependency...`);
        const dependencyResult = await createSchemaBasedEntity(collectionName, {
          displayName: `${collection} ${field.fieldName} dependency`
        });

        if (dependencyResult.success && dependencyResult.id) {
          entityData[field.fieldName] = dependencyResult.id;
          console.log(`✅ SchemaFactory: Created ${collectionName}/${dependencyResult.id} for ${field.fieldName}`);
        } else {
          throw new Error(`Failed to create ${collectionName}: ${dependencyResult.error}`);
        }
      }
    }
  }
}

/**
 * Generic factory interface - no hardcoded entity types
 */
export const schemaFactory = {
  /**
   * Create any entity type using schema-driven generation
   */
  create: (collection: string, overrides?: Record<string, unknown>): Promise<FactoryResult> =>
    createSchemaBasedEntity(collection, overrides),

  /**
   * Delete entity for cleanup
   */
  async delete(collection: string, id: string): Promise<boolean> {
    try {
      const result = runJtagCommand(`data/delete --collection="${collection}" --id="${id}"`);
      return result.deleted === true;
    } catch (error) {
      console.error(`Failed to delete ${collection}/${id}:`, error);
      return false;
    }
  },

  /**
   * Clear schema cache (useful for testing)
   */
  clearCache(): void {
    schemaCache.clear();
  }
};

// Backwards compatibility export
// eslint-disable-next-line @typescript-eslint/naming-convention
export const SchemaFactory = schemaFactory;