/**
 * User Create Persona Command - Shared Types
 *
 * Creates new PersonaUser instances for AI chat participation with LoRA training
 */

import type { JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { BaseUser } from '../../../../domain/user/BaseUser';
import type { UserData } from '../../../../system/data/domains/User';
import type { PersonaStyle } from '../../../../domain/user/PersonaUser';
import type { AIModelConfig } from '../../../../domain/user/UserRelationships';

/**
 * User Create Persona Parameters
 */
export interface UserCreatePersonaParams extends JTAGPayload {
  readonly displayName: string;
  readonly sessionId: UUID;
  readonly personaStyle: PersonaStyle;
  readonly systemPrompt?: string;
  readonly modelConfig?: AIModelConfig;
}

/**
 * User Create Persona Result
 */
export interface UserCreatePersonaResult extends JTAGPayload {
  readonly success: boolean;
  readonly userId?: UUID;
  readonly userData?: UserData; // For UI widgets
  readonly timestamp: string;
  readonly error?: string;
}

/**
 * Factory function for creating UserCreatePersonaParams
 */
export const createUserCreatePersonaParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<UserCreatePersonaParams, 'context' | 'sessionId'>
): UserCreatePersonaParams => createPayload(context, sessionId, data);

/**
 * Transform params to result
 */
export const createUserCreatePersonaResultFromParams = (
  params: UserCreatePersonaParams,
  differences: Omit<Partial<UserCreatePersonaResult>, 'context' | 'sessionId'>
): UserCreatePersonaResult => transformPayload(params, {
  success: false,
  timestamp: new Date().toISOString(),
  ...differences
});