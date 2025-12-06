/**
 * Hello Command - Shared Types
 *
 * A simple greeting command for testing the generator
 */

import type { CommandParams, CommandResult, JTAGContext } from '../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

/**
 * Hello Command Parameters
 */
export interface HelloParams extends CommandParams {
  // The name to greet
  name: string;
  // Add emoji to the greeting
  emoji?: boolean;
}

/**
 * Factory function for creating HelloParams
 */
export const createHelloParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // The name to greet
    name: string;
    // Add emoji to the greeting
    emoji?: boolean;
  }
): HelloParams => createPayload(context, sessionId, {
  emoji: data.emoji ?? false,
  ...data
});

/**
 * Hello Command Result
 */
export interface HelloResult extends CommandResult {
  success: boolean;
  // The generated greeting message
  greeting: string;
  // When the greeting was generated
  timestamp: number;
  error?: JTAGError;
}

/**
 * Factory function for creating HelloResult with defaults
 */
export const createHelloResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // The generated greeting message
    greeting?: string;
    // When the greeting was generated
    timestamp?: number;
    error?: JTAGError;
  }
): HelloResult => createPayload(context, sessionId, {
  greeting: data.greeting ?? '',
  timestamp: data.timestamp ?? 0,
  ...data
});

/**
 * Smart Hello-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createHelloResultFromParams = (
  params: HelloParams,
  differences: Omit<HelloResult, 'context' | 'sessionId'>
): HelloResult => transformPayload(params, differences);
