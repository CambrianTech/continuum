/**
 * Logging Status Command - Shared Types
 *
 * Show current logging configuration for all personas or a specific persona
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Logging Status Command Parameters
 */
export interface LoggingStatusParams extends CommandParams {
  // Specific persona to show status for. If not specified, shows all personas
  persona?: string;
}

/**
 * Factory function for creating LoggingStatusParams
 */
export const createLoggingStatusParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Specific persona to show status for. If not specified, shows all personas
    persona?: string;
  }
): LoggingStatusParams => createPayload(context, sessionId, {
  persona: data.persona ?? '',
  ...data
});

/**
 * Logging Status Command Result
 */
export interface LoggingStatusResult extends CommandResult {
  success: boolean;
  // Array of persona logging statuses with { persona, enabled, categories }
  personas: object[];
  // Whether system logging is enabled
  systemEnabled: boolean;
  // Default enabled state for unconfigured personas
  defaultEnabled: boolean;
  // List of valid category names
  availableCategories: string[];
  // Human-readable summary of logging state
  summary: string;
  error?: JTAGError;
}

/**
 * Factory function for creating LoggingStatusResult with defaults
 */
export const createLoggingStatusResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Array of persona logging statuses with { persona, enabled, categories }
    personas?: object[];
    // Whether system logging is enabled
    systemEnabled?: boolean;
    // Default enabled state for unconfigured personas
    defaultEnabled?: boolean;
    // List of valid category names
    availableCategories?: string[];
    // Human-readable summary of logging state
    summary?: string;
    error?: JTAGError;
  }
): LoggingStatusResult => createPayload(context, sessionId, {
  personas: data.personas ?? [],
  systemEnabled: data.systemEnabled ?? false,
  defaultEnabled: data.defaultEnabled ?? false,
  availableCategories: data.availableCategories ?? [],
  summary: data.summary ?? '',
  ...data
});

/**
 * Smart Logging Status-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createLoggingStatusResultFromParams = (
  params: LoggingStatusParams,
  differences: Omit<LoggingStatusResult, 'context' | 'sessionId'>
): LoggingStatusResult => transformPayload(params, differences);

/**
 * Logging Status — Type-safe command executor
 *
 * Usage:
 *   import { LoggingStatus } from '...shared/LoggingStatusTypes';
 *   const result = await LoggingStatus.execute({ ... });
 */
export const LoggingStatus = {
  execute(params: CommandInput<LoggingStatusParams>): Promise<LoggingStatusResult> {
    return Commands.execute<LoggingStatusParams, LoggingStatusResult>('logging/status', params as Partial<LoggingStatusParams>);
  },
  commandName: 'logging/status' as const,
} as const;
