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

/** Delay between seed operations to prevent IPC pool exhaustion */
const SEED_DELAY_MS = parseInt(process.env.SEED_DELAY_MS || '200');
const SEED_RETRIES = parseInt(process.env.SEED_RETRIES || '3');

function delay(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

/** Execute a jtag command with retry and delay */
export async function execWithRetry(cmd: string, retries = SEED_RETRIES): Promise<{ stdout: string; stderr: string }> {
  for (let attempt = 0; attempt <= retries; attempt++) {
    try {
      if (attempt > 0) {
        await delay(SEED_DELAY_MS * attempt); // Exponential backoff
      }
      const result = await execAsync(cmd, { timeout: 15000 });
      await delay(SEED_DELAY_MS); // Pace between operations
      return result;
    } catch (error: unknown) {
      const err = toExecError(error);
      // If stdout has success, it worked despite exit code
      if (err.stdout && err.stdout.includes('"success": true')) {
        await delay(SEED_DELAY_MS);
        return { stdout: err.stdout, stderr: err.stderr || '' };
      }
      if (attempt === retries) throw error;
      console.warn(`  ⏳ Retry ${attempt + 1}/${retries} for: ${cmd.substring(0, 80)}...`);
    }
  }
  throw new Error('Unreachable');
}

/** Run a jtag command and return parsed JSON result */
async function jtag(command: string, args: Record<string, string | number | boolean> = {}): Promise<{ success: boolean; data?: unknown; stdout: string }> {
  const argStr = Object.entries(args)
    .map(([k, v]) => typeof v === 'string' ? `--${k}='${v.replace(/'/g, `'"'"'`)}'` : `--${k}=${v}`)
    .join(' ');
  const cmd = `./jtag ${command} ${argStr}`;
  const { stdout } = await execWithRetry(cmd);
  try {
    const parsed = JSON.parse(stdout);
    return { success: parsed.success !== false, data: parsed.data ?? parsed, stdout };
  } catch {
    return { success: stdout.includes('"success": true'), stdout };
  }
}

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
    const result = await execWithRetry(cmd);
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
    console.error(`❌ Failed to create ${collection} ${displayName || id}:`);
    console.error(`   Error: ${err.message}`);
    return false;
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
    const result = await execWithRetry(cmd);
    const success = result.stdout.includes('"success": true');

    if (success) {
      console.log(`✅ Created ${collection} (state): ${displayName || id}${userId ? ` for user ${userId.slice(0, 8)}...` : ''}`);
      return true;
    } else {
      console.error(`❌ Failed to create ${collection} ${displayName || id}: Command returned unsuccessful result`);
      return false;
    }
  } catch (error: unknown) {
    const err = toExecError(error);
    const hasSuccess = err.stdout && err.stdout.includes('"success": true');

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
    const { stdout } = await execWithRetry(cmd);
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
    const { stdout: checkOut } = await execWithRetry(`./jtag ${DATA_COMMANDS.LIST} --collection=user_profiles --filter='${checkFilter}' --limit=1`);
    const checkResult = JSON.parse(checkOut);

    if (checkResult.success && checkResult.items?.length > 0) {
      // Profile exists — update it
      const existingId = checkResult.items[0].id;
      const updateProfileData = { bio: profile.bio, speciality: profile.speciality, visualIdentity };
      const updateArg = JSON.stringify(updateProfileData).replace(/'/g, `'"'"'`);
      await execWithRetry(`./jtag ${DATA_COMMANDS.UPDATE} --collection=user_profiles --id=${existingId} --data='${updateArg}'`);
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
    const { stdout } = await execWithRetry(profileCmd);
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
    const { stdout } = await execWithRetry(cmd);
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
    const { stdout } = await execWithRetry(cmd);
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
    const { stdout } = await execWithRetry(cmd);
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

  // Retry up to 3 times with backoff — Rust IPC may not be ready on first attempt
  const MAX_RETRIES = 3;
  for (let attempt = 1; attempt <= MAX_RETRIES; attempt++) {
    try {
      const { stdout } = await execWithRetry(cmd);
      const response: UserCreateResult = JSON.parse(stdout);

      if (response.success && response.user) {
        console.log(`✅ Created user (${type}): ${displayName} (uniqueId: ${response.user.uniqueId}, ID: ${response.user.id.slice(0, 8)}...)`);
        return response.user;
      } else if (response.error?.includes('IPC') || response.error?.includes('timeout')) {
        // Rust IPC not ready — retry
        if (attempt < MAX_RETRIES) {
          console.log(`   ⏳ Rust IPC not ready, retrying in ${attempt * 3}s (attempt ${attempt}/${MAX_RETRIES})...`);
          await new Promise(r => setTimeout(r, attempt * 3000));
          continue;
        }
      }
      console.error(`❌ Failed to create user ${displayName}: ${response.error || 'Unknown error'}`);
      return null;
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

      // "Record already exists" means the user is there — load it instead of failing
      if (err.stdout && err.stdout.includes('already exists')) {
        console.log(`✅ User ${displayName} already exists — loading`);
        if (uniqueId) {
          return await loadUserByUniqueId(uniqueId);
        }
        if (type === 'human') {
          return await loadFirstUserByType('human');
        }
      }

      // IPC/timeout errors — retry if attempts remain
      const isTransient = err.message?.includes('IPC') || err.message?.includes('timeout') || err.message?.includes('TIMEOUT');
      if (isTransient && attempt < MAX_RETRIES) {
        console.log(`   ⏳ Transient error, retrying in ${attempt * 3}s (attempt ${attempt}/${MAX_RETRIES})...`);
        await new Promise(r => setTimeout(r, attempt * 3000));
        continue;
      }

      console.error(`❌ Failed to create user ${displayName}: ${err.message}`);
      if (err.stdout) console.error(`   Output: ${err.stdout.substring(0, 500)}`);
      if (err.stderr) console.error(`   Stderr: ${err.stderr.substring(0, 500)}`);
      return null;
    }
  }
  return null; // All retries exhausted
}

/**
 * Load the first user of a given type (e.g., 'human')
 */
async function loadFirstUserByType(type: string): Promise<UserEntity | null> {
  try {
    const { stdout } = await execWithRetry(`./jtag ${DATA_COMMANDS.LIST} --collection=${UserEntity.collection} --filter='{"type":"${type}"}' --limit=1`);
    const response = JSON.parse(stdout);
    if (response.success && response.items?.length > 0) {
      return response.items[0] as UserEntity;
    }
  } catch { /* ignore */ }
  return null;
}

/**
 * Load an existing user by uniqueId
 */
export async function loadUserByUniqueId(uniqueId: string): Promise<UserEntity | null> {
  try {
    const { stdout } = await execWithRetry(`./jtag ${DATA_COMMANDS.LIST} --collection=${UserEntity.collection} --filter='{"uniqueId":"${uniqueId}"}'`);
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
