/**
 * Generate/Audit Command - Shared Types
 *
 * Audit generated modules for issues and optionally fix them
 */

import type { CommandParams, CommandResult, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { AuditReport } from '@generator/audit/AuditTypes';

/**
 * Generate/Audit Command Parameters
 */
export interface GenerateAuditParams extends CommandParams {
  // Specific module path to audit (e.g., "commands/hello")
  module?: string;
  // Module type to audit all of (e.g., "command")
  type?: 'command' | 'widget' | 'daemon';
  // Apply automatic fixes
  fix?: boolean;
  // Hibernate modules that can't be fixed
  hibernateFailures?: boolean;
}

/**
 * Factory function for creating GenerateAuditParams
 */
export const createGenerateAuditParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<GenerateAuditParams>, 'context' | 'sessionId'>
): GenerateAuditParams => createPayload(context, sessionId, data);

/**
 * Generate/Audit Command Result
 */
export interface GenerateAuditResult extends CommandResult {
  success: boolean;
  // Audit reports for each module audited
  reports: AuditReport[];
  // Summary statistics
  summary: {
    modulesAudited: number;
    totalErrors: number;
    totalWarnings: number;
    totalFixed: number;
  };
  error?: string;
}

/**
 * Factory function for creating GenerateAuditResult with defaults
 */
export const createGenerateAuditResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<GenerateAuditResult>, 'context' | 'sessionId'>
): GenerateAuditResult => createPayload(context, sessionId, {
  success: false,
  reports: [],
  summary: {
    modulesAudited: 0,
    totalErrors: 0,
    totalWarnings: 0,
    totalFixed: 0,
  },
  ...data
});

/**
 * Smart Generate/Audit-specific inheritance from params
 */
export const createGenerateAuditResultFromParams = (
  params: GenerateAuditParams,
  differences: Omit<Partial<GenerateAuditResult>, 'context' | 'sessionId'>
): GenerateAuditResult => transformPayload(params, {
  success: false,
  reports: [],
  summary: {
    modulesAudited: 0,
    totalErrors: 0,
    totalWarnings: 0,
    totalFixed: 0,
  },
  ...differences
});
