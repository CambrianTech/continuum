/**
 * Backfill Vectors Command - Shared Types
 *
 * Batch generates vector embeddings for existing records in a collection.
 */

import type { CommandParams, JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { UniversalFilter } from '../../../../daemons/data-daemon/shared/DataStorageAdapter';

/**
 * Backfill vectors command parameters
 */
export interface BackfillVectorsParams extends CommandParams {
  readonly collection: string;
  readonly textField: string;                    // Field to generate embeddings from (e.g., 'content')
  readonly filter?: UniversalFilter;             // Only backfill matching records
  readonly batchSize?: number;                   // Process N records at a time (default: 100)
  readonly model?: string;                       // Model name: 'all-minilm' | 'nomic-embed-text'
  readonly provider?: string;                    // Provider: 'ollama' | 'openai'
  readonly skipExisting?: boolean;               // Skip records that already have embeddings (default: true)
}

/**
 * Backfill vectors command result
 */
export interface BackfillVectorsResult extends JTAGPayload {
  readonly success: boolean;
  readonly vectorsCreated?: number;
  readonly vectorsSkipped?: number;
  readonly vectorsFailed?: number;
  readonly elapsedTime?: number;                 // ms
  readonly model?: {
    readonly name: string;
    readonly dimensions: number;
    readonly provider: string;
  };
  readonly error?: string;
}

export const createBackfillVectorsParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<BackfillVectorsParams, 'context' | 'sessionId'>
): BackfillVectorsParams => createPayload(context, sessionId, data);

export const createBackfillVectorsResultFromParams = (
  params: BackfillVectorsParams,
  differences: Omit<Partial<BackfillVectorsResult>, 'context' | 'sessionId'>
): BackfillVectorsResult => transformPayload(params, {
  success: false,
  ...differences
});
