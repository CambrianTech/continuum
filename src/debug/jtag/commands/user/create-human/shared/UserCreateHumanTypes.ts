/**
 * User Create Human Command - Shared Types
 *
 * Creates new HumanUser instances for chat participation and authentication
 */

import type { JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { BaseUser } from '../../../../domain/user/BaseUser';
import type { UserData } from '../../../../system/data/domains/User';

/**
 * User Create Human Parameters
 */
export interface UserCreateHumanParams extends JTAGPayload {
  readonly displayName: string;
  readonly sessionId: UUID;
}

/**
 * User Create Human Result
 */
export interface UserCreateHumanResult extends JTAGPayload {
  readonly success: boolean;
  readonly userId?: UUID;
  readonly userData?: UserData; // For UI widgets
  readonly timestamp: string;
  readonly error?: string;
}

/**
 * Factory function for creating UserCreateHumanParams
 */
export const createUserCreateHumanParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<UserCreateHumanParams, 'context' | 'sessionId'>
): UserCreateHumanParams => createPayload(context, sessionId, data);

/**
 * Transform params to result
 */
export const createUserCreateHumanResultFromParams = (
  params: UserCreateHumanParams,
  differences: Omit<Partial<UserCreateHumanResult>, 'context' | 'sessionId'>
): UserCreateHumanResult => transformPayload(params, {
  success: false,
  timestamp: new Date().toISOString(),
  ...differences
});