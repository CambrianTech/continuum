/**
 * Persona Learning Pattern Endorse Command - Shared Types
 *
 * Report the outcome of using a pattern. Updates confidence scores and can trigger validation or deprecation.
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../../system/core/shared/Commands';

/**
 * Persona Learning Pattern Endorse Command Parameters
 */
export interface PersonaLearningPatternEndorseParams extends CommandParams {
  // UUID of the pattern being endorsed
  patternId: string;
  // Was the pattern successful (true) or did it fail (false)
  success: boolean;
  // Optional notes about the usage experience
  notes?: string;
}

/**
 * Factory function for creating PersonaLearningPatternEndorseParams
 */
export const createPersonaLearningPatternEndorseParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // UUID of the pattern being endorsed
    patternId: string;
    // Was the pattern successful (true) or did it fail (false)
    success: boolean;
    // Optional notes about the usage experience
    notes?: string;
  }
): PersonaLearningPatternEndorseParams => createPayload(context, sessionId, {
  notes: data.notes ?? '',
  ...data
});

/**
 * Persona Learning Pattern Endorse Command Result
 */
export interface PersonaLearningPatternEndorseResult extends CommandResult {
  success: boolean;
  // Pattern that was endorsed
  patternId: string;
  // Confidence before endorsement
  previousConfidence: number;
  // Confidence after endorsement
  newConfidence: number;
  // Whether the pattern status changed
  statusChanged: boolean;
  // Current pattern status
  newStatus: string;
  // Description of what happened
  message: string;
  // Whether pattern is now eligible for LoRA training
  trainingCandidate: boolean;
  error?: JTAGError;
}

/**
 * Factory function for creating PersonaLearningPatternEndorseResult with defaults
 */
export const createPersonaLearningPatternEndorseResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Pattern that was endorsed
    patternId?: string;
    // Confidence before endorsement
    previousConfidence?: number;
    // Confidence after endorsement
    newConfidence?: number;
    // Whether the pattern status changed
    statusChanged?: boolean;
    // Current pattern status
    newStatus?: string;
    // Description of what happened
    message?: string;
    // Whether pattern is now eligible for LoRA training
    trainingCandidate?: boolean;
    error?: JTAGError;
  }
): PersonaLearningPatternEndorseResult => createPayload(context, sessionId, {
  patternId: data.patternId ?? '',
  previousConfidence: data.previousConfidence ?? 0,
  newConfidence: data.newConfidence ?? 0,
  statusChanged: data.statusChanged ?? false,
  newStatus: data.newStatus ?? '',
  message: data.message ?? '',
  trainingCandidate: data.trainingCandidate ?? false,
  ...data
});

/**
 * Smart Persona Learning Pattern Endorse-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createPersonaLearningPatternEndorseResultFromParams = (
  params: PersonaLearningPatternEndorseParams,
  differences: Omit<PersonaLearningPatternEndorseResult, 'context' | 'sessionId'>
): PersonaLearningPatternEndorseResult => transformPayload(params, differences);

/**
 * PersonaLearningPatternEndorse — Type-safe command executor
 *
 * Usage:
 *   import { PersonaLearningPatternEndorse } from '...shared/PersonaLearningPatternEndorseTypes';
 *   const result = await PersonaLearningPatternEndorse.execute({ ... });
 */
export const PersonaLearningPatternEndorse = {
  execute(params: CommandInput<PersonaLearningPatternEndorseParams>): Promise<PersonaLearningPatternEndorseResult> {
    return Commands.execute<PersonaLearningPatternEndorseParams, PersonaLearningPatternEndorseResult>('persona/learning/pattern/endorse', params as Partial<PersonaLearningPatternEndorseParams>);
  },
  commandName: 'persona/learning/pattern/endorse' as const,
} as const;
