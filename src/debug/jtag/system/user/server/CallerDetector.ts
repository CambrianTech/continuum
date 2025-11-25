/**
 * Caller Detector Utility
 *
 * Runtime detection of caller type from JTAGContext for caller-adaptive command output.
 * Enables commands to determine who is calling (persona/human/script) and adapt their output accordingly.
 *
 * @see docs/CALLER-ADAPTIVE-OUTPUTS.md for architecture details
 */

import type { JTAGContext, CallerType, CallerCapabilities } from '../core/types/JTAGTypes';
import type { UUID } from '../core/types/CrossPlatformUUID';
import { DataDaemon } from '../../daemons/data-daemon/shared/DataDaemon';
import { COLLECTIONS } from '../data/config/DatabaseConfig';
import type { UserEntity } from '../data/entities/UserEntity';

/**
 * Detect caller type from JTAGContext and userId
 *
 * Detection Strategy:
 * 1. Check explicit callerType hint in context (if provided)
 * 2. Look up user by userId parameter
 * 3. Determine type based on user.entity.type:
 *    - type='persona' → 'persona' (PersonaUser instances)
 *    - type='human' → 'human' (HumanUser instances)
 *    - type='agent' | type='system' → 'script' (conservative default)
 * 4. If user lookup fails, default to 'script' (safest fallback)
 *
 * @param context - JTAG execution context with optional callerType hint
 * @param userId - User UUID to look up
 * @returns CallerType indicating who is calling ('persona' | 'human' | 'script')
 */
export async function detectCallerType(context: JTAGContext, userId: UUID): Promise<CallerType> {
  // 1. Check explicit hint first (fastest path)
  if (context.callerType) {
    return context.callerType;
  }

  // 2. Look up user by userId
  try {
    const userResult = await DataDaemon.read<UserEntity>(COLLECTIONS.USERS, userId);

    if (!userResult.success || !userResult.data) {
      console.warn(`CallerDetector: User not found for userId=${userId}, defaulting to 'script'`);
      return 'script';
    }

    const user = userResult.data.data;

    // 3. Map UserEntity.type to CallerType
    switch (user.type) {
      case 'persona':
        return 'persona';
      case 'human':
        return 'human';
      case 'agent':
      case 'system':
        // Conservative: treat agents and system users as scripts
        return 'script';
      default:
        console.warn(`CallerDetector: Unknown user type '${user.type}', defaulting to 'script'`);
        return 'script';
    }
  } catch (error) {
    console.error(`CallerDetector: Error looking up user ${userId}:`, error);
    return 'script'; // Safe fallback
  }
}

/**
 * Get caller capabilities from user configuration
 *
 * Looks up capabilities from user's configuration (vision, audio, parsing support).
 * Currently reads from UserEntity.mediaConfig and other relevant fields.
 *
 * Future Enhancement: Could be extended to negotiate capabilities dynamically.
 *
 * @param userId - User UUID to look up capabilities for
 * @returns CallerCapabilities indicating what the caller can process
 */
export async function getCallerCapabilities(userId: UUID): Promise<CallerCapabilities> {
  try {
    const userResult = await DataDaemon.read<UserEntity>(COLLECTIONS.USERS, userId);

    if (!userResult.success || !userResult.data) {
      console.warn(`CallerDetector: User not found for userId=${userId}, returning default capabilities`);
      return getDefaultCapabilities();
    }

    const user = userResult.data.data;

    // Build capabilities from user configuration
    const capabilities: CallerCapabilities = {};

    // Check vision support from mediaConfig
    if (user.mediaConfig?.supportedMediaTypes?.includes('image')) {
      capabilities.vision = true;
    }

    // Check audio support from mediaConfig
    if (user.mediaConfig?.supportedMediaTypes?.includes('audio')) {
      capabilities.audio = true;
    }

    // Check parsing support (personas with code analysis capabilities)
    if (user.type === 'persona' && user.capabilities?.canTrain) {
      // PersonaUsers with training capability can parse structured code
      capabilities.parsing = true;
    }

    // Determine display environment
    if (user.type === 'human') {
      capabilities.display = 'browser'; // HumanUsers typically use browser UI
    } else if (user.type === 'persona' || user.type === 'agent') {
      capabilities.display = 'none'; // AI users don't have display
    } else {
      capabilities.display = 'terminal'; // Scripts/system users may use terminal
    }

    return capabilities;
  } catch (error) {
    console.error(`CallerDetector: Error looking up capabilities for user ${userId}:`, error);
    return getDefaultCapabilities(); // Safe fallback
  }
}

/**
 * Get default capabilities (fallback when user lookup fails)
 */
function getDefaultCapabilities(): CallerCapabilities {
  return {
    vision: false,
    audio: false,
    parsing: false,
    display: 'none'
  };
}
