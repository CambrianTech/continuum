/**
 * Seed Helper Functions
 *
 * Utility functions for creating and managing seed data via JTAG commands.
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import { UserEntity } from '../../system/data/entities/UserEntity';
import type { UserCreateResult } from '../../commands/user/create/shared/UserCreateTypes';

const execAsync = promisify(exec);

/**
 * Create a record via JTAG ${DATA_COMMANDS.CREATE} command with proper shell escaping
 */
export async function createRecord(
  collection: string,
  data: any,
  id: string,
  displayName?: string,
  userId?: string
): Promise<boolean> {
  const dataArg = JSON.stringify(data).replace(/'/g, `'"'"'`);
  const cmd = `./jtag ${DATA_COMMANDS.CREATE} --collection=${collection} --data='${dataArg}'`;

  try {
    const result = await execAsync(cmd);
    const success = result.stdout.includes('"success": true');

    if (success) {
      console.log(`✅ Created ${collection}: ${displayName || id}`);
      return true;
    } else {
      console.error(`❌ Failed to create ${collection} ${displayName || id}: Command returned unsuccessful result`);
      console.error(`Response: ${result.stdout}`);
      return false;
    }
  } catch (error: any) {
    const hasSuccess = error.stdout && error.stdout.includes('"success": true');

    if (hasSuccess) {
      console.log(`✅ Created ${collection}: ${displayName || id}`);
      return true;
    } else {
      console.error(`❌ Failed to create ${collection} ${displayName || id}:`);
      console.error(`   Error: ${error.message}`);
      if (error.stdout) console.error(`   Output: ${error.stdout.substring(0, 500)}...`);
      if (error.stderr) console.error(`   Stderr: ${error.stderr.substring(0, 500)}...`);
      return false;
    }
  }
}

/**
 * Create a state record (UserState, ContentType, etc.)
 */
export async function createStateRecord(
  collection: string,
  data: any,
  id: string,
  userId?: string,
  displayName?: string
): Promise<boolean> {
  const dataArg = JSON.stringify(data).replace(/'/g, `'\"'\"'`);
  const cmd = `./jtag ${DATA_COMMANDS.CREATE} --collection=${collection} --data='${dataArg}'`;

  try {
    const result = await execAsync(cmd);
    const success = result.stdout.includes('\"success\": true');

    if (success) {
      console.log(`✅ Created ${collection} (state): ${displayName || id}${userId ? ` for user ${userId.slice(0, 8)}...` : ''}`);
      return true;
    } else {
      console.error(`❌ Failed to create ${collection} ${displayName || id}: Command returned unsuccessful result`);
      console.error(`Response: ${result.stdout}`);
      return false;
    }
  } catch (error: any) {
    const hasSuccess = error.stdout && error.stdout.includes('\"success\": true');

    if (hasSuccess) {
      console.log(`✅ Created ${collection} (state): ${displayName || id}${userId ? ` for user ${userId.slice(0, 8)}...` : ''}`);
      return true;
    } else {
      console.error(`❌ Failed to create ${collection} ${displayName || id}:`);
      console.error(`   Error: ${error.message}`);
      if (error.stdout) console.error(`   Output: ${error.stdout.substring(0, 500)}...`);
      if (error.stderr) console.error(`   Stderr: ${error.stderr.substring(0, 500)}...`);
      return false;
    }
  }
}

/**
 * Update persona bio via shortDescription field
 */
export async function updatePersonaProfile(
  userId: string,
  profile: { bio: string; speciality: string }
): Promise<boolean> {
  const updateData = {
    shortDescription: profile.bio
  };
  const dataArg = JSON.stringify(updateData).replace(/'/g, `'"'"'`);
  const cmd = `./jtag ${DATA_COMMANDS.UPDATE} --collection=users --id=${userId} --data='${dataArg}'`;

  try {
    const { stdout } = await execAsync(cmd);
    const result = JSON.parse(stdout);

    if (result.success) {
      console.log(`  ✅ Updated persona bio for user ${userId.slice(0, 8)}...`);
      return true;
    } else {
      console.error(`  ❌ Failed to update persona bio: ${result.error || 'Unknown error'}`);
      return false;
    }
  } catch (error: any) {
    console.error(`  ❌ Failed to update persona bio: ${error.message}`);
    return false;
  }
}

/**
 * Update persona configuration for intelligent resource management
 */
export async function updatePersonaConfig(userId: string, config: any): Promise<boolean> {
  const configArg = JSON.stringify(config).replace(/'/g, `'"'"'`);
  const updateData = { personaConfig: config };
  const dataArg = JSON.stringify(updateData).replace(/'/g, `'"'"'`);
  const cmd = `./jtag ${DATA_COMMANDS.UPDATE} --collection=users --id=${userId} --data='${dataArg}'`;

  try {
    const { stdout } = await execAsync(cmd);
    const result = JSON.parse(stdout);

    if (result.success) {
      console.log(`  ✅ Updated persona config for user ${userId.slice(0, 8)}...`);
      return true;
    } else {
      console.error(`  ❌ Failed to update persona config: ${result.error || 'Unknown error'}`);
      return false;
    }
  } catch (error: any) {
    console.error(`  ❌ Failed to update persona config: ${error.message}`);
    return false;
  }
}

/**
 * Update user metadata with audio-native model info
 * Sets modelId and isAudioNative flags for VoiceOrchestrator routing
 */
export async function updateUserMetadata(
  userId: string,
  metadata: { modelId?: string; isAudioNative?: boolean }
): Promise<boolean> {
  const updateData = { metadata };
  const dataArg = JSON.stringify(updateData).replace(/'/g, `'"'"'`);
  const cmd = `./jtag ${DATA_COMMANDS.UPDATE} --collection=users --id=${userId} --data='${dataArg}'`;

  try {
    const { stdout } = await execAsync(cmd);
    const result = JSON.parse(stdout);

    if (result.success) {
      console.log(`  ✅ Updated metadata for user ${userId.slice(0, 8)}... (modelId: ${metadata.modelId})`);
      return true;
    } else {
      console.error(`  ❌ Failed to update metadata: ${result.error || 'Unknown error'}`);
      return false;
    }
  } catch (error: any) {
    console.error(`  ❌ Failed to update metadata: ${error.message}`);
    return false;
  }
}

/**
 * Create a user via user/create command (proper factory-based creation)
 *
 * Note: Pass uniqueId from persona config (clean slug without @ prefix)
 */
export async function createUserViaCommand(
  type: 'human' | 'agent' | 'persona',
  displayName: string,
  uniqueId?: string,
  provider?: string
): Promise<UserEntity | null> {
  const uniqueIdArg = uniqueId ? ` --uniqueId=${uniqueId}` : '';
  const providerArg = provider ? ` --provider=${provider}` : '';
  const cmd = `./jtag user/create --type=${type} --displayName="${displayName}"${uniqueIdArg}${providerArg}`;

  try {
    const { stdout } = await execAsync(cmd);
    const response: UserCreateResult = JSON.parse(stdout);

    if (response.success && response.user) {
      console.log(`✅ Created user (${type}): ${displayName} (uniqueId: ${response.user.uniqueId}, ID: ${response.user.id.slice(0, 8)}...)`);
      return response.user;
    } else {
      console.error(`❌ Failed to create user ${displayName}: ${response.error || 'Unknown error'}`);
      return null;
    }
  } catch (error: any) {
    if (error.stdout) {
      try {
        const response: UserCreateResult = JSON.parse(error.stdout);
        if (response.success && response.user) {
          console.log(`✅ Created user (${type}): ${displayName} (uniqueId: ${response.user.uniqueId}, ID: ${response.user.id.slice(0, 8)}...)`);
          return response.user;
        }
      } catch (parseError) {
        // Fall through
      }
    }

    console.error(`❌ Failed to create user ${displayName}: ${error.message}`);
    if (error.stdout) console.error(`   Output: ${error.stdout.substring(0, 500)}`);
    if (error.stderr) console.error(`   Stderr: ${error.stderr.substring(0, 500)}`);
    return null;
  }
}

/**
 * Load an existing user by uniqueId
 */
export async function loadUserByUniqueId(uniqueId: string): Promise<UserEntity | null> {
  try {
    const { stdout } = await execAsync(`./jtag ${DATA_COMMANDS.LIST} --collection=${UserEntity.collection} --filter='{"uniqueId":"${uniqueId}"}'`);
    const response = JSON.parse(stdout);

    if (response.success && response.items && response.items.length > 0) {
      const user = response.items[0];
      console.log(`✅ Loaded existing user: ${user.displayName} (uniqueId: ${uniqueId}, ID: ${user.id.slice(0, 8)}...)`);
      return user;
    } else {
      console.log(`⚠️ User with uniqueId ${uniqueId} not found in database`);
      return null;
    }
  } catch (error: any) {
    console.error(`❌ Failed to load user with uniqueId ${uniqueId}: ${error.message}`);
    if (error.stdout) console.error(`   Output: ${error.stdout.substring(0, 500)}`);
    return null;
  }
}

/**
 * Seed multiple records of the same type
 */
export async function seedRecords<T extends { id: string; displayName?: string }>(
  collection: string,
  records: T[],
  getDisplayName?: (record: T) => string,
  getUserId?: (record: T) => string
): Promise<void> {
  console.log(`📝 Creating ${records.length} ${collection} records via ${DATA_COMMANDS.CREATE}...`);

  let successCount = 0;
  for (const record of records) {
    const displayName = getDisplayName ? getDisplayName(record) : record.displayName || record.id;
    const userId = getUserId ? getUserId(record) : undefined;
    const success = await createRecord(collection, record, record.id, displayName, userId);
    if (success) successCount++;
  }

  console.log(`📊 Created ${successCount}/${records.length} ${collection} records`);

  if (successCount < records.length) {
    throw new Error(`❌ Failed to create all ${collection} records - only ${successCount}/${records.length} succeeded`);
  }
}
