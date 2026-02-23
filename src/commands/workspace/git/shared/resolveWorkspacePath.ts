/**
 * resolveWorkspacePath - Resolve workspace path from userId (UUID)
 *
 * Shared utility for git workspace commands that need to find a persona's
 * workspace directory. Looks up the user entity to get the human-readable
 * uniqueId, then constructs the path using that (not the UUID).
 *
 * Path convention: .continuum/sessions/user/shared/{uniqueId}/workspace
 */

import { ORM } from '@daemons/data-daemon/server/ORM';
import { COLLECTIONS } from '@system/data/config/DatabaseConfig';
import type { UserEntity } from '@system/data/entities/UserEntity';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import * as path from 'path';

/**
 * Resolve workspace path from a userId (UUID).
 * Looks up the user entity to get uniqueId for human-readable directory naming.
 * Falls back to userId if entity lookup fails.
 */
export async function resolveWorkspacePathFromUserId(userId: UUID): Promise<string> {
  let dirName: string = userId; // fallback to UUID if lookup fails

  try {
    const entity = await ORM.read<UserEntity>(COLLECTIONS.USERS, userId, 'default');
    if (entity?.uniqueId) {
      dirName = entity.uniqueId;
    }
  } catch {
    // Entity lookup failed — use UUID as fallback
  }

  return path.resolve(
    process.cwd(),
    '.continuum/sessions/user/shared',
    dirName,
    'workspace',
  );
}
