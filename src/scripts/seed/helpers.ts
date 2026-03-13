/**
 * Seed Helper Functions
 *
 * Utility functions for creating and managing seed data via JTAG commands.
 */

import { exec } from 'child_process';
import { promisify } from 'util';
import { UserEntity } from '../../system/data/entities/UserEntity';
import type { UserCreateResult } from '../../commands/user/create/shared/UserCreateTypes';
import { DATA_COMMANDS } from '../../commands/data/shared/DataCommandConstants';

const execAsync = promisify(exec);

/** Child process exec error — has stdout/stderr from the failed command */
interface ExecError extends Error {
  stdout?: string;
  stderr?: string;
  code?: number;
}

/** Narrow unknown catch to ExecError (child_process always throws ExecException) */
function toExecError(error: unknown): ExecError {
  if (error instanceof Error) return error as ExecError;
  return new Error(String(error)) as ExecError;
}

/**
 * Create a record via JTAG ${DATA_COMMANDS.CREATE} command with proper shell escaping
 */
export async function createRecord(
  collection: string,
  data: Record<string, unknown>,
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
  } catch (error: unknown) {
    const err = toExecError(error);
    const hasSuccess = err.stdout && err.stdout.includes('"success": true');

    if (hasSuccess) {
      console.log(`✅ Created ${collection}: ${displayName || id}`);
      return true;
    } else {
      console.error(`❌ Failed to create ${collection} ${displayName || id}:`);
      console.error(`   Error: ${err.message}`);
      if (err.stdout) console.error(`   Output: ${err.stdout.substring(0, 500)}...`);
      if (err.stderr) console.error(`   Stderr: ${err.stderr.substring(0, 500)}...`);
      return false;
    }
  }
}

/**
 * Create a state record (UserState, ContentType, etc.)
 */
export async function createStateRecord(
  collection: string,
  data: Record<string, unknown>,
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
  } catch (error: unknown) {
    const err = toExecError(error);
    const hasSuccess = err.stdout && err.stdout.includes('\"success\": true');

    if (hasSuccess) {
      console.log(`✅ Created ${collection} (state): ${displayName || id}${userId ? ` for user ${userId.slice(0, 8)}...` : ''}`);
      return true;
    } else {
      console.error(`❌ Failed to create ${collection} ${displayName || id}:`);
      console.error(`   Error: ${err.message}`);
      if (err.stdout) console.error(`   Output: ${err.stdout.substring(0, 500)}...`);
      if (err.stderr) console.error(`   Stderr: ${err.stderr.substring(0, 500)}...`);
      return false;
    }
  }
}

/**
 * Update persona bio via shortDescription field AND create UserProfileEntity
 */
export async function updatePersonaProfile(
  userId: string,
  profile: { bio: string; speciality: string; accentColor?: string }
): Promise<boolean> {
  // Update shortDescription on UserEntity
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
    } else {
      console.error(`  ❌ Failed to update persona bio: ${result.error || 'Unknown error'}`);
      return false;
    }
  } catch (error: unknown) {
    console.error(`  ❌ Failed to update persona bio: ${toExecError(error).message}`);
    return false;
  }

  // Ensure UserProfileEntity exists (check first, create or update)
  const visualIdentity = {
    avatar: '',
    theme: 'dark',
    accentColor: profile.accentColor || '#00d4ff'
  };
  const preferences = {
    language: 'en',
    timezone: 'UTC',
    notifications: { mentions: true, directMessages: true, roomUpdates: false },
    privacy: { showOnlineStatus: true, allowDirectMessages: true, shareActivity: false }
  };

  // Check if profile already exists
  try {
    const checkFilter = JSON.stringify({ userId }).replace(/'/g, `'"'"'`);
    const { stdout: checkOut } = await execAsync(`./jtag ${DATA_COMMANDS.LIST} --collection=user_profiles --filter='${checkFilter}' --limit=1`);
    const checkResult = JSON.parse(checkOut);

    if (checkResult.success && checkResult.items?.length > 0) {
      // Profile exists — update it
      const existingId = checkResult.items[0].id;
      const updateProfileData = { bio: profile.bio, speciality: profile.speciality, visualIdentity };
      const updateArg = JSON.stringify(updateProfileData).replace(/'/g, `'"'"'`);
      await execAsync(`./jtag ${DATA_COMMANDS.UPDATE} --collection=user_profiles --id=${existingId} --data='${updateArg}'`);
      console.log(`  ✅ Updated profile entity for user ${userId.slice(0, 8)}...`);
      return true;
    }
  } catch {
    // Check failed — fall through to create
  }

  // Profile doesn't exist — create it
  const profileData = {
    userId,
    bio: profile.bio,
    speciality: profile.speciality,
    joinedAt: new Date().toISOString(),
    visualIdentity,
    preferences
  };
  const profileArg = JSON.stringify(profileData).replace(/'/g, `'"'"'`);
  const profileCmd = `./jtag ${DATA_COMMANDS.CREATE} --collection=user_profiles --data='${profileArg}'`;

  try {
    const { stdout } = await execAsync(profileCmd);
    if (stdout.includes('"success"')) {
      console.log(`  ✅ Created profile entity for user ${userId.slice(0, 8)}...`);
      return true;
    }
  } catch (error: unknown) {
    const err = toExecError(error);
    if (err.stdout?.includes('"success"')) {
      console.log(`  ✅ Created profile entity for user ${userId.slice(0, 8)}...`);
      return true;
    }
    console.error(`  ⚠️ Failed to create profile entity: ${err.message}`);
  }

  return true;
}

/**
 * Update persona configuration for intelligent resource management
 */
export async function updatePersonaConfig(userId: string, config: Record<string, unknown>): Promise<boolean> {
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
  } catch (error: unknown) {
    console.error(`  ❌ Failed to update persona config: ${toExecError(error).message}`);
    return false;
  }
}

/**
 * Update user's modelConfig.provider field
 * Used when seeding to ensure existing users get updated providers
 */
export async function updateUserModelConfig(
  userId: string,
  provider: string
): Promise<boolean> {
  const updateData = { modelConfig: { provider } };
  const dataArg = JSON.stringify(updateData).replace(/'/g, `'"'"'`);
  const cmd = `./jtag ${DATA_COMMANDS.UPDATE} --collection=users --id=${userId} --data='${dataArg}'`;

  try {
    const { stdout } = await execAsync(cmd);
    const result = JSON.parse(stdout);

    if (result.success) {
      console.log(`  ✅ Updated modelConfig.provider to '${provider}' for user ${userId.slice(0, 8)}...`);
      return true;
    } else {
      console.error(`  ❌ Failed to update modelConfig: ${result.error || 'Unknown error'}`);
      return false;
    }
  } catch (error: unknown) {
    console.error(`  ❌ Failed to update modelConfig: ${toExecError(error).message}`);
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
  } catch (error: unknown) {
    console.error(`  ❌ Failed to update metadata: ${toExecError(error).message}`);
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
  } catch (error: unknown) {
    const err = toExecError(error);
    if (err.stdout) {
      try {
        const response: UserCreateResult = JSON.parse(err.stdout);
        if (response.success && response.user) {
          console.log(`✅ Created user (${type}): ${displayName} (uniqueId: ${response.user.uniqueId}, ID: ${response.user.id.slice(0, 8)}...)`);
          return response.user;
        }
      } catch {
        // Fall through
      }
    }

    console.error(`❌ Failed to create user ${displayName}: ${err.message}`);
    if (err.stdout) console.error(`   Output: ${err.stdout.substring(0, 500)}`);
    if (err.stderr) console.error(`   Stderr: ${err.stderr.substring(0, 500)}`);
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
  } catch (error: unknown) {
    const err = toExecError(error);
    console.error(`❌ Failed to load user with uniqueId ${uniqueId}: ${err.message}`);
    if (err.stdout) console.error(`   Output: ${err.stdout.substring(0, 500)}`);
    return null;
  }
}

/**
 * Seed multiple records of the same type
 */
export async function seedRecords<T extends Record<string, unknown> & { id: string; displayName?: string }>(
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
