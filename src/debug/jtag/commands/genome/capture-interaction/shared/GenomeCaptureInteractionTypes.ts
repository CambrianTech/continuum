/**
 * GenomeCaptureInteractionTypes - Capture AI interactions for continuous learning
 *
 * Called during recipe execution to record inputs/outputs for LoRA training.
 * Accumulates examples in-memory for batch micro-tuning.
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';

/**
 * Parameters for genome/capture-interaction command
 */
export interface GenomeCaptureInteractionParams extends CommandParams {
  /**
   * Which role/persona is this interaction for
   */
  roleId: string;

  /**
   * PersonaUser ID (if different from current context)
   */
  personaId?: UUID;

  /**
   * Learning domain (typescript, conversation, chess, etc.)
   */
  domain: string;

  /**
   * LoRA adapter to update (defaults to domain-based)
   */
  loraAdapter?: string;

  /**
   * Input to the AI (RAG context, prompt, game state, etc.)
   */
  input: string;

  /**
   * Output from the AI (response, code, move, etc.)
   */
  output: string;

  /**
   * Thought stream (internal reasoning, if captured)
   */
  thoughtStream?: string;

  /**
   * Additional context metadata
   */
  metadata?: {
    roomId?: UUID;
    messageId?: UUID;
    contextId?: UUID;
    recipeId?: UUID;
    timestamp?: string;
    [key: string]: unknown;
  };
}

/**
 * Result from genome/capture-interaction command
 */
export interface GenomeCaptureInteractionResult extends CommandResult {
  success: boolean;
  error?: string;

  /**
   * Capture summary
   */
  capture?: {
    exampleId: UUID;
    domain: string;
    roleId: string;
    bufferSize: number;           // How many examples in buffer now
    readyForTraining: boolean;    // Has batch threshold been reached?
  };
}
