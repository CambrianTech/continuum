/**
 * Entity Factory - Simple test utilities for easy entity creation
 *
 * Provides smart defaults and factory methods for all entity types
 */

import { execSync } from 'child_process';

export interface FactoryEntityData {
  [key: string]: unknown;
}

/**
 * Smart defaults for different entity types
 */
const ENTITY_DEFAULTS = {
  User: {
    type: 'human',
    status: 'online',
    lastActiveAt: () => new Date().toISOString(),
    capabilities: {},
    sessionsActive: {}
  },
  Room: {
    displayName: () => `Test Room ${Date.now()}`,
    name: () => `Test Room ${Date.now()}`,
    description: 'A test room for development',
    type: 'public',
    status: 'active',
    memberCount: 1,
    lastActivityAt: () => new Date().toISOString(),
    lastMessageAt: () => new Date().toISOString(),
    privacy: 'public',
    // Smart default - will be populated with test user ID when creating
    ownerId: null, // Will be auto-populated
    settings: {
      allowInvites: true,
      requireApproval: false,
      maxMembers: 100
    },
    members: [],
    tags: []
  },
  ChatMessage: {
    content: () => `Test message ${Date.now()}`,
    type: 'text',
    status: 'sent',
    sentAt: () => new Date().toISOString(),
    metadata: {}
  }
} as const;

/**
 * Create entity with smart defaults using JTAG commands
 */
export async function createTestEntity(
  collection: string,
  overrides: FactoryEntityData = {}
): Promise<{ success: boolean; id?: string; error?: string }> {
  try {
    // Get defaults for this collection
    const defaults = ENTITY_DEFAULTS[collection as keyof typeof ENTITY_DEFAULTS] || {};

    // Build entity data
    const entityData: FactoryEntityData = {};

    // Apply defaults, resolving functions
    for (const [key, value] of Object.entries(defaults)) {
      if (typeof value === 'function') {
        entityData[key] = value();
      } else {
        entityData[key] = value;
      }
    }

    // Add collection-specific test fields and handle dependencies
    if (collection === 'User') {
      entityData.displayName = `Test User ${Date.now()}`;
      entityData.shortDescription = 'Test user for development';
    } else if (collection === 'Room') {
      // Room needs an owner - create a test user if not provided
      if (!overrides.ownerId && entityData.ownerId === null) {
        console.log('🏭 EntityFactory: Room needs ownerId, creating test user...');
        const ownerResult = await createTestEntity('User', { displayName: 'Room Owner' });
        if (ownerResult.success && ownerResult.id) {
          entityData.ownerId = ownerResult.id;
          console.log(`🏭 EntityFactory: Created owner ${ownerResult.id} for room`);
        } else {
          return {
            success: false,
            error: 'Failed to create room owner: ' + ownerResult.error
          };
        }
      }
    }

    // Apply user overrides
    Object.assign(entityData, overrides);

    console.log(`🏭 EntityFactory: Creating ${collection} with data:`, entityData);

    // Use JTAG command to create - handle both success and error cases
    const command = `./jtag data/create --collection="${collection}" --data='${JSON.stringify(entityData)}'`;
    let output: string;

    try {
      output = execSync(command, {
        encoding: 'utf8',
        cwd: process.cwd(),
        stdio: 'pipe'
      });
    } catch (error: any) {
      // execSync throws on non-zero exit codes, but JTAG might still return valid JSON
      output = error.stdout || '';
    }

    // Parse the JSON response from output
    const jsonStart = output.indexOf('{');
    if (jsonStart >= 0) {
      // Extract JSON properly by counting braces
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

      const jsonStr = output.substring(jsonStart, jsonEnd);
      const result = JSON.parse(jsonStr);

      if (result.success && result.data?.id) {
        console.log(`✅ EntityFactory: Created ${collection}/${result.data.id}`);
        return {
          success: true,
          id: result.data.id
        };
      } else {
        return {
          success: false,
          error: result.error || 'Entity creation failed'
        };
      }
    }

    return {
      success: false,
      error: 'Could not parse command response'
    };

  } catch (error) {
    console.error(`❌ EntityFactory: Failed to create ${collection}:`, error);
    return {
      success: false,
      error: error instanceof Error ? error.message : String(error)
    };
  }
}

/**
 * Quick factory methods for common entities
 */
export const EntityFactory = {
  User: (overrides?: FactoryEntityData) => createTestEntity('User', overrides),
  Room: (overrides?: FactoryEntityData) => createTestEntity('Room', overrides),
  ChatMessage: (overrides?: FactoryEntityData) => createTestEntity('ChatMessage', overrides),

  /**
   * Create multiple entities of the same type
   */
  async createMultiple(collection: string, count: number, baseOverrides?: FactoryEntityData): Promise<string[]> {
    const ids: string[] = [];

    for (let i = 0; i < count; i++) {
      const overrides = {
        ...baseOverrides,
        displayName: `${baseOverrides?.displayName || 'Test'} ${i + 1}`
      };

      const result = await createTestEntity(collection, overrides);
      if (result.success && result.id) {
        ids.push(result.id);
      } else {
        console.error(`Failed to create ${collection} ${i + 1}:`, result.error);
      }
    }

    return ids;
  },

  /**
   * Clean up test entities
   */
  async cleanup(collection: string, id: string): Promise<boolean> {
    try {
      const command = `./jtag data/delete --collection="${collection}" --id="${id}"`;
      let output: string;

      try {
        output = execSync(command, {
          encoding: 'utf8',
          cwd: process.cwd(),
          stdio: 'pipe'
        });
      } catch (error: any) {
        // execSync throws on non-zero exit codes, but JTAG might still return valid JSON
        output = error.stdout || '';
      }

      const jsonStart = output.indexOf('{');
      if (jsonStart >= 0) {
        // Extract JSON properly by counting braces
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

        const jsonStr = output.substring(jsonStart, jsonEnd);
        const result = JSON.parse(jsonStr);
        console.log(`🧹 EntityFactory: Cleanup ${collection}/${id} - deleted: ${result.deleted}`);
        return result.deleted === true;
      }

      return false;
    } catch (error) {
      console.error(`Failed to cleanup ${collection}/${id}:`, error);
      return false;
    }
  }
};