/**
 * Persona Learning Pattern Capture Command - Shared Types
 *
 * Capture a successful pattern for cross-AI learning. When an AI discovers a working solution, they share it with the team.
 */

import type { CommandParams, CommandResult, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Persona Learning Pattern Capture Command Parameters
 */
export interface PersonaLearningPatternCaptureParams extends CommandParams {
  // Short descriptive name for the pattern
  name: string;
  // Pattern type: debugging, tool-use, optimization, architecture, communication, other
  type: string;
  // Domain where pattern applies: chat, code, tools, web, general
  domain: string;
  // The problem this pattern solves
  problem: string;
  // How the pattern solves the problem
  solution: string;
  // Detailed description of the pattern
  description?: string;
  // Searchable tags for pattern discovery
  tags?: string[];
  // Conditions when this pattern should be used
  applicableWhen?: string[];
  // Example usages of the pattern
  examples?: string[];
  // Make pattern immediately visible to all AIs (default: false, pending validation)
  makePublic?: boolean;
}

/**
 * Factory function for creating PersonaLearningPatternCaptureParams
 */
export const createPersonaLearningPatternCaptureParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Short descriptive name for the pattern
    name: string;
    // Pattern type: debugging, tool-use, optimization, architecture, communication, other
    type: string;
    // Domain where pattern applies: chat, code, tools, web, general
    domain: string;
    // The problem this pattern solves
    problem: string;
    // How the pattern solves the problem
    solution: string;
    // Detailed description of the pattern
    description?: string;
    // Searchable tags for pattern discovery
    tags?: string[];
    // Conditions when this pattern should be used
    applicableWhen?: string[];
    // Example usages of the pattern
    examples?: string[];
    // Make pattern immediately visible to all AIs (default: false, pending validation)
    makePublic?: boolean;
  }
): PersonaLearningPatternCaptureParams => createPayload(context, sessionId, {
  description: data.description ?? '',
  tags: data.tags ?? undefined,
  applicableWhen: data.applicableWhen ?? undefined,
  examples: data.examples ?? undefined,
  makePublic: data.makePublic ?? false,
  ...data
});

/**
 * Persona Learning Pattern Capture Command Result
 */
export interface PersonaLearningPatternCaptureResult extends CommandResult {
  success: boolean;
  // UUID of the created pattern
  patternId: string;
  // Pattern name
  name: string;
  // Initial status: pending or active
  status: string;
  // Initial confidence score (0.5)
  confidence: number;
  // Next steps guidance
  message: string;
  error?: JTAGError;
}

/**
 * Factory function for creating PersonaLearningPatternCaptureResult with defaults
 */
export const createPersonaLearningPatternCaptureResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // UUID of the created pattern
    patternId?: string;
    // Pattern name
    name?: string;
    // Initial status: pending or active
    status?: string;
    // Initial confidence score (0.5)
    confidence?: number;
    // Next steps guidance
    message?: string;
    error?: JTAGError;
  }
): PersonaLearningPatternCaptureResult => createPayload(context, sessionId, {
  patternId: data.patternId ?? '',
  name: data.name ?? '',
  status: data.status ?? '',
  confidence: data.confidence ?? 0,
  message: data.message ?? '',
  ...data
});

/**
 * Smart Persona Learning Pattern Capture-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createPersonaLearningPatternCaptureResultFromParams = (
  params: PersonaLearningPatternCaptureParams,
  differences: Omit<PersonaLearningPatternCaptureResult, 'context' | 'sessionId'>
): PersonaLearningPatternCaptureResult => transformPayload(params, differences);
