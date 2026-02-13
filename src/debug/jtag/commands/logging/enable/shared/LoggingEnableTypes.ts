/**
 * Logging Enable Command - Shared Types
 *
 * Enable logging for a persona. Persists to .continuum/logging.json
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Logging Enable Command Parameters
 */
export interface LoggingEnableParams extends CommandParams {
  // Persona uniqueId to enable logging for (e.g., 'helper', 'codereview')
  persona: string;
  // Specific category to enable (e.g., 'cognition', 'hippocampus'). If not specified, enables all categories
  category?: string;
}

/**
 * Factory function for creating LoggingEnableParams
 */
export const createLoggingEnableParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Persona uniqueId to enable logging for (e.g., 'helper', 'codereview')
    persona: string;
    // Specific category to enable (e.g., 'cognition', 'hippocampus'). If not specified, enables all categories
    category?: string;
  }
): LoggingEnableParams => createPayload(context, sessionId, {
  category: data.category ?? '',
  ...data
});

/**
 * Logging Enable Command Result
 */
export interface LoggingEnableResult extends CommandResult {
  success: boolean;
  // The persona that was enabled
  persona: string;
  // Categories now enabled for this persona
  categories: string[];
  // Human-readable status message
  message: string;
  error?: JTAGError;
}

/**
 * Factory function for creating LoggingEnableResult with defaults
 */
export const createLoggingEnableResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // The persona that was enabled
    persona?: string;
    // Categories now enabled for this persona
    categories?: string[];
    // Human-readable status message
    message?: string;
    error?: JTAGError;
  }
): LoggingEnableResult => createPayload(context, sessionId, {
  persona: data.persona ?? '',
  categories: data.categories ?? [],
  message: data.message ?? '',
  ...data
});

/**
 * Smart Logging Enable-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createLoggingEnableResultFromParams = (
  params: LoggingEnableParams,
  differences: Omit<LoggingEnableResult, 'context' | 'sessionId'>
): LoggingEnableResult => transformPayload(params, differences);

/**
 * Logging Enable — Type-safe command executor
 *
 * Usage:
 *   import { LoggingEnable } from '...shared/LoggingEnableTypes';
 *   const result = await LoggingEnable.execute({ ... });
 */
export const LoggingEnable = {
  execute(params: CommandInput<LoggingEnableParams>): Promise<LoggingEnableResult> {
    return Commands.execute<LoggingEnableParams, LoggingEnableResult>('logging/enable', params as Partial<LoggingEnableParams>);
  },
  commandName: 'logging/enable' as const,
} as const;
