/**
 * Ai Context Slice Command - Shared Types
 *
 * Retrieve full content of a context item by ID - companion to context/search for getting complete entity data
 */

import type { CommandParams, CommandResult, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { CollectionName } from '../../../context/search/shared/AiContextSearchTypes';
import { Commands } from '../../../../../system/core/shared/Commands';

// Re-export for convenience
export type { CollectionName } from '../../../context/search/shared/AiContextSearchTypes';

/**
 * Full entity content (not truncated like search results)
 */
export interface ContextSliceItem {
  /** Entity ID */
  readonly id: UUID;
  /** Collection name */
  readonly collection: CollectionName;
  /** Full content */
  readonly content: string;
  /** When this happened */
  readonly timestamp: string;
  /** Source context */
  readonly source: string;
  /** Full metadata */
  readonly metadata: Record<string, unknown>;
  /** Related items (if requested) */
  readonly related?: ContextSliceItem[];
}

/**
 * Ai Context Slice Command Parameters
 */
export interface AiContextSliceParams extends CommandParams {
  // Entity ID to retrieve
  id: string;
  // Entity type: chat_messages, memories, timeline_events, tool_results
  type: string;
  // Persona ID for per-persona database lookup
  personaId?: string;
  // Include related items (replies, thread context)
  includeRelated?: boolean;
  // Max related items to include (default: 5)
  relatedLimit?: number;
}

/**
 * Factory function for creating AiContextSliceParams
 */
export const createAiContextSliceParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Entity ID to retrieve
    id: string;
    // Entity type: chat_messages, memories, timeline_events, tool_results
    type: string;
    // Persona ID for per-persona database lookup
    personaId?: string;
    // Include related items (replies, thread context)
    includeRelated?: boolean;
    // Max related items to include (default: 5)
    relatedLimit?: number;
  }
): AiContextSliceParams => createPayload(context, sessionId, {
  personaId: data.personaId ?? '',
  includeRelated: data.includeRelated ?? false,
  relatedLimit: data.relatedLimit ?? 0,
  ...data
});

/**
 * Ai Context Slice Command Result
 */
export interface AiContextSliceResult extends CommandResult {
  success: boolean;
  // The full entity content or null if not found
  item: ContextSliceItem | null;
  // Retrieval time in milliseconds
  durationMs: number;
  error?: JTAGError;
}

/**
 * Factory function for creating AiContextSliceResult with defaults
 */
export const createAiContextSliceResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // The full entity content or null if not found
    item?: ContextSliceItem | null;
    // Retrieval time in milliseconds
    durationMs?: number;
    error?: JTAGError;
  }
): AiContextSliceResult => createPayload(context, sessionId, {
  item: data.item ?? null,
  durationMs: data.durationMs ?? 0,
  ...data
});

/**
 * Smart Ai Context Slice-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAiContextSliceResultFromParams = (
  params: AiContextSliceParams,
  differences: Omit<AiContextSliceResult, 'context' | 'sessionId'>
): AiContextSliceResult => transformPayload(params, differences);

/**
 * AiContextSlice — Type-safe command executor
 *
 * Usage:
 *   import { AiContextSlice } from '...shared/AiContextSliceTypes';
 *   const result = await AiContextSlice.execute({ ... });
 */
export const AiContextSlice = {
  execute(params: CommandInput<AiContextSliceParams>): Promise<AiContextSliceResult> {
    return Commands.execute<AiContextSliceParams, AiContextSliceResult>('ai/context/slice', params as Partial<AiContextSliceParams>);
  },
  commandName: 'ai/context/slice' as const,
} as const;
