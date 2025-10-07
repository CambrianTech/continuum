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
  // Find JSON start - use first complete JSON object (not nested sub-objects)
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
      cwd: process.cwd(),
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
 * Create complete entity data structures following the seeding approach
 * This mirrors the exact structure used in successful seeding
 */
function createCompleteEntityData(collection: string, timestamp: number, overrides: Record<string, unknown> = {}): Record<string, unknown> {
  const now = new Date().toISOString();

  switch (collection) {
    case 'User':
    case 'users':  // Support both entity class name and collection name
      // Mirror the exact successful seeding structure
      return {
        displayName: overrides.displayName || `Test User ${timestamp}`,
        shortDescription: overrides.shortDescription || "Test user entity",
        uniqueId: overrides.uniqueId || `test-user-${timestamp}`,
        type: overrides.type || "human",
        profile: {
          displayName: overrides.displayName || `Test User ${timestamp}`,
          avatar: "🧪",
          bio: "Test user for integration testing",
          location: "Test Environment",
          joinedAt: now
        },
        capabilities: {
          canSendMessages: true,
          canReceiveMessages: true,
          canTrain: false,
          canCreateRooms: true,
          canInviteOthers: true,
          canModerate: true,
          autoResponds: false,
          providesContext: false,
          canAccessPersonas: true,
        },
        preferences: {
          theme: 'dark',
          language: 'en',
          timezone: 'UTC',
          notifications: {
            mentions: true,
            directMessages: true,
            roomUpdates: false
          },
          privacy: {
            showOnlineStatus: true,
            allowDirectMessages: true,
            shareActivity: false
          }
        },
        status: "online",
        lastActiveAt: now,
        sessionsActive: [],
        ...overrides
      };

    case 'Room':
    case 'rooms':  // Support both entity class name and collection name
      // Mirror the exact successful seeding structure
      return {
        name: (overrides.name || `test-room-${timestamp}`).toLowerCase(), // Ensure lowercase like seeding
        uniqueId: overrides.uniqueId || `test-room-${timestamp}`,  // uniqueId is required
        displayName: overrides.displayName || overrides.name || `Test Room ${timestamp}`,
        description: overrides.description || "Test room for integration testing",
        topic: "Test room topic for development",
        type: "public",
        status: "active",
        ownerId: "002350cc-0031-408d-8040-004f000f", // Use actual seeded user ID
        lastMessageAt: now,
        privacy: {
          isPublic: true,
          requiresInvite: false,
          allowGuestAccess: true,
          searchable: true
        },
        settings: {
          allowReactions: true,
          allowThreads: true,
          allowFileSharing: true,
          messageRetentionDays: 365
        },
        stats: {
          memberCount: 1,
          messageCount: 0,
          createdAt: now,
          lastActivityAt: now
        },
        members: [],
        tags: ["test", "integration"],
        ...overrides
      };

    case 'ChatMessage':
    case 'chat_messages':  // Support both entity class name and collection name
      // Mirror the exact successful seeding structure
      return {
        roomId: overrides.roomId || "5e71a0c8-0303-4eb8-a478-3a121248", // Use actual seeded room ID
        senderId: overrides.senderId || "002350cc-0031-408d-8040-004f000f", // Use actual seeded user ID
        senderName: overrides.senderName || "Test Sender",
        senderType: overrides.senderType || "user",  // senderType is required
        content: overrides.content || {
          text: `Test message ${timestamp}`,
          attachments: [],
          formatting: {
            markdown: false,
            mentions: [],
            hashtags: [],
            links: [],
            codeBlocks: []
          }
        },
        status: "sent",
        priority: "normal",
        timestamp: now,
        reactions: [],
        ...overrides
      };

    default:
      throw new Error(`Unknown collection: ${collection}`);
  }
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
      return example || now;

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
      if (field.fieldName === 'uniqueId') {
        return `test-unique-${timestamp}`;
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
 * Create entity with complete structure following successful seeding approach
 */
export async function createCompleteEntity(
  collection: string,
  overrides: Record<string, unknown> = {}
): Promise<FactoryResult> {
  try {
    console.log(`🏗️ SchemaFactory: Creating ${collection} with complete structure (seeding approach)...`);

    const timestamp = Date.now();

    // Use the complete entity data structure that mirrors successful seeding
    const entityData = createCompleteEntityData(collection, timestamp, overrides);

    console.log(`🏗️ SchemaFactory: Generated complete data for ${collection}:`, entityData);

    // Special case: User entities must use user/create command (not data/create)
    let result: Record<string, unknown>;
    if (collection === 'User' || collection === 'users') {
      const displayName = entityData.displayName as string;
      const type = (entityData.type as string) || 'human';
      const uniqueId = (entityData.uniqueId as string) || `test-user-${timestamp}`;

      result = runJtagCommand(`user/create --type=${type} --displayName="${displayName}" --uniqueId="${uniqueId}"`);

      // user/create returns { success, user } instead of { success, data }
      if (result.success && result.user) {
        const userData = result.user as Record<string, unknown>;
        console.log(`✅ SchemaFactory: Created User/${userData.id} via user/create`);
        return {
          success: true,
          id: userData.id as string,
          data: userData
        };
      }
    } else {
      // All other entities use data/create
      result = runJtagCommand(`data/create --collection="${collection}" --data='${JSON.stringify(entityData)}'`);

      if (result.success && result.data?.id) {
        console.log(`✅ SchemaFactory: Created ${collection}/${result.data.id}`);
        return {
          success: true,
          id: result.data.id as string,
          data: result.data as Record<string, unknown>
        };
      }
    }

    return {
      success: false,
      error: result.error as string ?? 'Entity creation failed'
    };

  } catch (error) {
    console.error(`❌ SchemaFactory: Failed to create ${collection}:`, error);
    return {
      success: false,
      error: error instanceof Error ? error.message : String(error)
    };
  }
}

/**
 * Create entity with schema-driven defaults (original approach, now fallback)
 */
export async function createSchemaBasedEntity(
  collection: string,
  overrides: Record<string, unknown> = {}
): Promise<FactoryResult> {
  // Try the complete entity approach first (mirrors successful seeding)
  try {
    return await createCompleteEntity(collection, overrides);
  } catch (error) {
    console.log(`🔄 SchemaFactory: Complete entity approach failed, falling back to schema-driven approach...`);
  }

  // Fallback to original schema-driven approach
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