/**
 * Persona Learning Pattern Query Command - Shared Types
 *
 * Query the collective pattern knowledge base. Search for patterns that might help solve the current problem.
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../../system/core/shared/Commands';

/**
 * Summary of a pattern for query results
 */
export interface PatternSummary {
  id: string;
  name: string;
  type: string;
  domain: string;
  problem: string;
  solution: string;
  confidence: number;
  status: string;
  successCount: number;
  failureCount: number;
  discoveredAt: string;
  sourcePersonaId: string;
  tags: string[];
}

/**
 * Persona Learning Pattern Query Command Parameters
 */
export interface PersonaLearningPatternQueryParams extends CommandParams {
  // Filter by domain: chat, code, tools, web, general
  domain?: string;
  // Filter by type: debugging, tool-use, optimization, architecture, communication
  type?: string;
  // Keywords to search for in pattern name/problem/solution
  keywords?: string[];
  // Free text search across all pattern fields
  search?: string;
  // Minimum confidence score 0-1 (default: 0.3)
  minConfidence?: number;
  // Filter by status: pending, validated, active, deprecated
  status?: string;
  // Max results to return (default: 10)
  limit?: number;
  // Sort by: confidence, successCount, discoveredAt (default: confidence)
  orderBy?: string;
}

/**
 * Factory function for creating PersonaLearningPatternQueryParams
 */
export const createPersonaLearningPatternQueryParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Filter by domain: chat, code, tools, web, general
    domain?: string;
    // Filter by type: debugging, tool-use, optimization, architecture, communication
    type?: string;
    // Keywords to search for in pattern name/problem/solution
    keywords?: string[];
    // Free text search across all pattern fields
    search?: string;
    // Minimum confidence score 0-1 (default: 0.3)
    minConfidence?: number;
    // Filter by status: pending, validated, active, deprecated
    status?: string;
    // Max results to return (default: 10)
    limit?: number;
    // Sort by: confidence, successCount, discoveredAt (default: confidence)
    orderBy?: string;
  }
): PersonaLearningPatternQueryParams => createPayload(context, sessionId, {
  domain: data.domain ?? '',
  type: data.type ?? '',
  keywords: data.keywords ?? undefined,
  search: data.search ?? '',
  minConfidence: data.minConfidence ?? 0,
  status: data.status ?? '',
  limit: data.limit ?? 0,
  orderBy: data.orderBy ?? '',
  ...data
});

/**
 * Persona Learning Pattern Query Command Result
 */
export interface PersonaLearningPatternQueryResult extends CommandResult {
  success: boolean;
  // Matching patterns with problem/solution
  patterns: PatternSummary[];
  // Total patterns matching the query
  totalMatches: number;
  // Usage guidance
  message: string;
  error?: JTAGError;
}

/**
 * Factory function for creating PersonaLearningPatternQueryResult with defaults
 */
export const createPersonaLearningPatternQueryResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Matching patterns with problem/solution
    patterns?: PatternSummary[];
    // Total patterns matching the query
    totalMatches?: number;
    // Usage guidance
    message?: string;
    error?: JTAGError;
  }
): PersonaLearningPatternQueryResult => createPayload(context, sessionId, {
  patterns: data.patterns ?? [],
  totalMatches: data.totalMatches ?? 0,
  message: data.message ?? '',
  ...data
});

/**
 * Smart Persona Learning Pattern Query-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createPersonaLearningPatternQueryResultFromParams = (
  params: PersonaLearningPatternQueryParams,
  differences: Omit<PersonaLearningPatternQueryResult, 'context' | 'sessionId'>
): PersonaLearningPatternQueryResult => transformPayload(params, differences);

/**
 * PersonaLearningPatternQuery — Type-safe command executor
 *
 * Usage:
 *   import { PersonaLearningPatternQuery } from '...shared/PersonaLearningPatternQueryTypes';
 *   const result = await PersonaLearningPatternQuery.execute({ ... });
 */
export const PersonaLearningPatternQuery = {
  execute(params: CommandInput<PersonaLearningPatternQueryParams>): Promise<PersonaLearningPatternQueryResult> {
    return Commands.execute<PersonaLearningPatternQueryParams, PersonaLearningPatternQueryResult>('persona/learning/pattern/query', params as Partial<PersonaLearningPatternQueryParams>);
  },
  commandName: 'persona/learning/pattern/query' as const,
} as const;
