/**
 * Hello Command - Shared Types
 *
 * Simple hello world command for testing
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Hello Command Parameters
 */
export interface HelloParams extends CommandParams {
  _noParams?: never; // Marker to avoid empty interface
}

/**
 * Factory function for creating HelloParams
 */
export const createHelloParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Record<string, never>
): HelloParams => createPayload(context, sessionId, {

  ...data
});

/**
 * Hello Command Result
 */
export interface HelloResult extends CommandResult {
  success: boolean;
  // Hello world message
  message: string;
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
    // Hello world message
    message?: string;
    error?: JTAGError;
  }
): HelloResult => createPayload(context, sessionId, {
  message: data.message ?? '',
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

/**
 * Hello — Type-safe command executor
 *
 * Usage:
 *   import { Hello } from '...shared/HelloTypes';
 *   const result = await Hello.execute({ ... });
 */
export const Hello = {
  execute(params: CommandInput<HelloParams>): Promise<HelloResult> {
    return Commands.execute<HelloParams, HelloResult>('utilities/hello', params as Partial<HelloParams>);
  },
  commandName: 'utilities/hello' as const,
} as const;
