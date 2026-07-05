/**
 * Logs Config Command - Shared Types
 *
 * Get or set logging configuration per persona and category
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { JTAGError } from '../../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { LoggingConfigData } from '../../../../system/core/logging/LoggingConfig';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Logs Config Command Parameters
 */
export interface LogsConfigParams extends CommandParams {
  // Persona uniqueId to get/set config for
  persona?: string;
  // Action: get (default), enable, disable
  action?: 'get' | 'enable' | 'disable';
  // Specific log category to enable/disable
  category?: string;
}

/**
 * Factory function for creating LogsConfigParams
 */
export const createLogsConfigParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Persona uniqueId to get/set config for
    persona?: string;
    // Action: get (default), enable, disable
    action?: 'get' | 'enable' | 'disable';
    // Specific log category to enable/disable
    category?: string;
  }
): LogsConfigParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  persona: data.persona ?? '',
  action: data.action ?? undefined,
  category: data.category ?? '',
  ...data
});

/**
 * Logs Config Command Result
 */
export interface PersonaLoggingStatus {
  persona: string;
  enabled: boolean;
  categories: string[];
  source: 'explicit' | 'default';
}

export interface LogsConfigResult extends CommandResult {
  success: boolean;
  // Full logging configuration
  config: LoggingConfigData;
  // Config for specific persona
  personaConfig: { enabled: boolean; categories: string[] };
  // Per-persona status list (for overview display)
  statuses?: PersonaLoggingStatus[];
  // Available categories
  availableCategories?: string[];
  // Status message
  message: string;
  error?: JTAGError;
}

/**
 * Factory function for creating LogsConfigResult with defaults
 */
export const createLogsConfigResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Full logging configuration
    config: LoggingConfigData;
    // Config for specific persona
    personaConfig: { enabled: boolean; categories: string[] };
    // Status message
    message?: string;
    error?: JTAGError;
  }
): LogsConfigResult => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data,
  message: data.message ?? ''
});

/**
 * Smart Logs Config-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createLogsConfigResultFromParams = (
  params: LogsConfigParams,
  differences: Omit<LogsConfigResult, 'context' | 'sessionId'>
): LogsConfigResult => transformPayload(params, differences);

/**
 * LogsConfig — Type-safe command executor
 *
 * Usage:
 *   import { LogsConfig } from '...shared/LogsConfigTypes';
 *   const result = await LogsConfig.execute({ ... });
 */
export const LogsConfig = {
  execute(params: CommandInput<LogsConfigParams>): Promise<LogsConfigResult> {
    return Commands.execute<LogsConfigParams, LogsConfigResult>('logs/config', params as Partial<LogsConfigParams>);
  },
  commandName: 'logs/config' as const,
} as const;
