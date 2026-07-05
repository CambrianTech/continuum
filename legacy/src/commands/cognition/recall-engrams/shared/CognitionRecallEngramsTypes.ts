/**
 * Cognition Recall Engrams Command - Shared Types
 *
 * Query a persona's admitted-engram store. Modes: 'recent' (default) returns newest-first N engrams; 'by_id' looks up by exact engram id; 'by_keyword' does case-insensitive substring match; 'by_origin' filters by EngramOriginKind (chat | airc | tool | self_reflection). Wraps the Rust IPC handler shipped in #1121 PR-5.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';


/**
 * Cognition Recall Engrams Command Parameters
 */
export interface CognitionRecallEngramsParams extends CommandParams {
  // UUID of the persona whose engram store to query
  personaId: string;
  // Recall mode (default: 'recent')
  kind?: 'recent' | 'by_id' | 'by_keyword' | 'by_origin';
  // Max engrams to return (default: 10). Ignored when kind='by_id'.
  limit?: number;
  // Engram UUID (required when kind='by_id')
  id?: string;
  // Substring to match against engram content (required when kind='by_keyword')
  keyword?: string;
  // Origin filter (required when kind='by_origin')
  origin?: 'chat' | 'airc' | 'tool' | 'self_reflection';
}

/**
 * Factory function for creating CognitionRecallEngramsParams
 */
export const createCognitionRecallEngramsParams = (
  context: JTAGContext,
  sessionId: UUID,
  userId: UUID,
  data: {
    // UUID of the persona whose engram store to query
    personaId: string;
    // Recall mode (default: 'recent')
    kind?: 'recent' | 'by_id' | 'by_keyword' | 'by_origin';
    // Max engrams to return (default: 10). Ignored when kind='by_id'.
    limit?: number;
    // Engram UUID (required when kind='by_id')
    id?: string;
    // Substring to match against engram content (required when kind='by_keyword')
    keyword?: string;
    // Origin filter (required when kind='by_origin')
    origin?: 'chat' | 'airc' | 'tool' | 'self_reflection';
  },
): CognitionRecallEngramsParams => createPayload(context, sessionId, {
  userId,
  kind: data.kind ?? undefined,
  limit: data.limit ?? 0,
  id: data.id ?? '',
  keyword: data.keyword ?? '',
  origin: data.origin ?? undefined,
  ...data,
});

/**
 * Cognition Recall Engrams Command Result
 */
export interface CognitionRecallEngramsResult extends CommandResult {
  success: boolean;
  // Matching engrams (typed as Engram in shared/generated/persona/Engram.ts)
  engrams: Array<Record<string, unknown>>;
  // Number of engrams returned
  count: number;
  error?: JTAGError;
}

/**
 * Factory function for creating CognitionRecallEngramsResult with defaults
 */
export const createCognitionRecallEngramsResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Matching engrams (typed as Engram in shared/generated/persona/Engram.ts)
    engrams: Array<Record<string, unknown>>;
    // Number of engrams returned
    count: number;
    error?: JTAGError;
  }
): CognitionRecallEngramsResult => createPayload(context, sessionId, {

  ...data
});

/**
 * Smart Cognition Recall Engrams-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createCognitionRecallEngramsResultFromParams = (
  params: CognitionRecallEngramsParams,
  differences: Omit<CognitionRecallEngramsResult, 'context' | 'sessionId' | 'userId'>
): CognitionRecallEngramsResult => transformPayload(params, differences);

/**
 * Cognition Recall Engrams — Type-safe command executor
 *
 * Usage:
 *   import { CognitionRecallEngrams } from '...shared/CognitionRecallEngramsTypes';
 *   const result = await CognitionRecallEngrams.execute({ ... });
 */
export const CognitionRecallEngrams = {
  execute(params: CommandInput<CognitionRecallEngramsParams>): Promise<CognitionRecallEngramsResult> {
    return Commands.execute<CognitionRecallEngramsParams, CognitionRecallEngramsResult>('cognition/recall-engrams', params as Partial<CognitionRecallEngramsParams>);
  },
  commandName: 'cognition/recall-engrams' as const,
} as const;
