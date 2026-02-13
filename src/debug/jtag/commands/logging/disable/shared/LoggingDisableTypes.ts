/**
 * Logging Disable Command - Shared Types
 *
 * Disable logging for a persona. Persists to .continuum/logging.json
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Logging Disable Command Parameters
 */
export interface LoggingDisableParams extends CommandParams {
  // Persona uniqueId to disable logging for (e.g., 'helper', 'codereview')
  persona: string;
  // Specific category to disable. If not specified, disables all logging for the persona
  category?: string;
}

/**
 * Factory function for creating LoggingDisableParams
 */
export const createLoggingDisableParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Persona uniqueId to disable logging for (e.g., 'helper', 'codereview')
    persona: string;
    // Specific category to disable. If not specified, disables all logging for the persona
    category?: string;
  }
): LoggingDisableParams => createPayload(context, sessionId, {
  category: data.category ?? '',
  ...data
});

/**
 * Logging Disable Command Result
 */
export interface LoggingDisableResult extends CommandResult {
  success: boolean;
  // The persona that was disabled
  persona: string;
  // Whether any logging remains enabled for this persona
  enabled: boolean;
  // Categories still enabled (empty if all disabled)
  categories: string[];
  // Human-readable status message
  message: string;
  error?: JTAGError;
}

/**
 * Factory function for creating LoggingDisableResult with defaults
 */
export const createLoggingDisableResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // The persona that was disabled
    persona?: string;
    // Whether any logging remains enabled for this persona
    enabled?: boolean;
    // Categories still enabled (empty if all disabled)
    categories?: string[];
    // Human-readable status message
    message?: string;
    error?: JTAGError;
  }
): LoggingDisableResult => createPayload(context, sessionId, {
  persona: data.persona ?? '',
  enabled: data.enabled ?? false,
  categories: data.categories ?? [],
  message: data.message ?? '',
  ...data
});

/**
 * Smart Logging Disable-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createLoggingDisableResultFromParams = (
  params: LoggingDisableParams,
  differences: Omit<LoggingDisableResult, 'context' | 'sessionId'>
): LoggingDisableResult => transformPayload(params, differences);

/**
 * Logging Disable — Type-safe command executor
 *
 * Usage:
 *   import { LoggingDisable } from '...shared/LoggingDisableTypes';
 *   const result = await LoggingDisable.execute({ ... });
 */
export const LoggingDisable = {
  execute(params: CommandInput<LoggingDisableParams>): Promise<LoggingDisableResult> {
    return Commands.execute<LoggingDisableParams, LoggingDisableResult>('logging/disable', params as Partial<LoggingDisableParams>);
  },
  commandName: 'logging/disable' as const,
} as const;
