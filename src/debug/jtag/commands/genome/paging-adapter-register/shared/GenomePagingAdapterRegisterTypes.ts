/**
 * Genome Paging Adapter Register Command Types
 *
 * Register a mock LoRA adapter in the global adapter registry.
 * This must be done before activating adapters for personas.
 *
 * Phase 7: Mock adapters only
 * Phase 8+: Real Ollama adapters
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';

export interface GenomePagingAdapterRegisterParams extends CommandParams {
  adapterId: UUID;
  name: string;
  domain: string;
  sizeMB: number;
  priority?: number;
}

export interface GenomePagingAdapterRegisterResult extends CommandResult {
  success: boolean;
  registered: boolean;
  adapterId: UUID;
  error?: string;
}

/**
 * Helper to create GenomePagingAdapterRegisterResult from params
 */
export const createGenomePagingAdapterRegisterResultFromParams = (
  params: GenomePagingAdapterRegisterParams,
  differences: Omit<Partial<GenomePagingAdapterRegisterResult>, 'context' | 'sessionId'>
): GenomePagingAdapterRegisterResult => transformPayload(params, {
  success: false,
  registered: false,
  adapterId: params.adapterId,
  ...differences
});
