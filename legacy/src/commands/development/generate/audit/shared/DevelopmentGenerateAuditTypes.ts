/**
 * Development Generate Audit Command - Shared Types
 *
 * Audit all commands for generator conformance. Scans every command directory and checks for: matching generator spec, static accessor (Name.execute pattern), factory functions, any casts in Types files. Reports conformance status and summary statistics.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Development Generate Audit Command Parameters
 */
export interface DevelopmentGenerateAuditParams extends CommandParams {
  // Output format: 'summary' (counts only), 'full' (every command, default), 'json' (machine-readable)
  format?: 'summary' | 'full' | 'json';
  // Filter to specific issue type. Omit for all commands.
  filter?: 'missing-spec' | 'missing-accessor' | 'has-any' | 'conformant';
}

/**
 * Factory function for creating DevelopmentGenerateAuditParams
 */
export const createDevelopmentGenerateAuditParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Output format: 'summary' (counts only), 'full' (every command, default), 'json' (machine-readable)
    format?: 'summary' | 'full' | 'json';
    // Filter to specific issue type. Omit for all commands.
    filter?: 'missing-spec' | 'missing-accessor' | 'has-any' | 'conformant';
  }
): DevelopmentGenerateAuditParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  format: data.format ?? undefined,
  filter: data.filter ?? undefined,
  ...data
});

/**
 * Development Generate Audit Command Result
 */
export interface DevelopmentGenerateAuditResult extends CommandResult {
  success: boolean;
  // Total number of command directories found
  totalCommands: number;
  // Commands that have matching generator specs
  withSpecs: number;
  // Commands missing the Name.execute() static accessor pattern
  missingAccessors: number;
  // Commands missing createParams/createResult factory functions
  missingFactories: number;
  // Total 'any' casts found across all command Types files
  totalAnyCasts: number;
  // Number of commands containing 'any' casts
  commandsWithAny: number;
  // Spec files with no matching command directory
  orphanedSpecs: string[];
  // Per-command audit details (when format='full' or 'json')
  entries: Array<{ commandName: string; hasSpec: boolean; hasStaticAccessor: boolean; hasFactoryFunctions: boolean; anyCastCount: number; issues: string[] }>;
  error?: JTAGError;
}

/**
 * Factory function for creating DevelopmentGenerateAuditResult with defaults
 */
export const createDevelopmentGenerateAuditResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Total number of command directories found
    totalCommands?: number;
    // Commands that have matching generator specs
    withSpecs?: number;
    // Commands missing the Name.execute() static accessor pattern
    missingAccessors?: number;
    // Commands missing createParams/createResult factory functions
    missingFactories?: number;
    // Total 'any' casts found across all command Types files
    totalAnyCasts?: number;
    // Number of commands containing 'any' casts
    commandsWithAny?: number;
    // Spec files with no matching command directory
    orphanedSpecs?: string[];
    // Per-command audit details (when format='full' or 'json')
    entries?: Array<{ commandName: string; hasSpec: boolean; hasStaticAccessor: boolean; hasFactoryFunctions: boolean; anyCastCount: number; issues: string[] }>;
    error?: JTAGError;
  }
): DevelopmentGenerateAuditResult => createPayload(context, sessionId, {
  totalCommands: data.totalCommands ?? 0,
  withSpecs: data.withSpecs ?? 0,
  missingAccessors: data.missingAccessors ?? 0,
  missingFactories: data.missingFactories ?? 0,
  totalAnyCasts: data.totalAnyCasts ?? 0,
  commandsWithAny: data.commandsWithAny ?? 0,
  orphanedSpecs: data.orphanedSpecs ?? [],
  entries: data.entries ?? [],
  ...data
});

/**
 * Smart Development Generate Audit-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createDevelopmentGenerateAuditResultFromParams = (
  params: DevelopmentGenerateAuditParams,
  differences: Omit<DevelopmentGenerateAuditResult, 'context' | 'sessionId' | 'userId'>
): DevelopmentGenerateAuditResult => transformPayload(params, differences);

/**
 * Development Generate Audit — Type-safe command executor
 *
 * Usage:
 *   import { DevelopmentGenerateAudit } from '...shared/DevelopmentGenerateAuditTypes';
 *   const result = await DevelopmentGenerateAudit.execute({ ... });
 */
export const DevelopmentGenerateAudit = {
  execute(params: CommandInput<DevelopmentGenerateAuditParams>): Promise<DevelopmentGenerateAuditResult> {
    return Commands.execute<DevelopmentGenerateAuditParams, DevelopmentGenerateAuditResult>('development/generate/audit', params as Partial<DevelopmentGenerateAuditParams>);
  },
  commandName: 'development/generate/audit' as const,
} as const;
