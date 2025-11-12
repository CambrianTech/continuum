/**
 * Genome Stats Command Types
 *
 * Get genome statistics - returns global daemon stats and per-persona stats.
 */

import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';

export interface GenomeStatsParams extends CommandParams {
  personaId?: UUID;  // Optional: get stats for specific persona
}

export interface PersonaGenomeInfo {
  personaId: UUID;
  displayName: string;
  memoryUsedMB: number;
  memoryQuotaMB: number;
  memoryUtilization: number;
  activeAdapters: UUID[];
  priority: number;
  loadCount: number;
  evictionCount: number;
  lastActivatedAt: number | null;
}

export interface GenomeStatsResult extends CommandResult {
  success: boolean;

  // Global stats
  globalMemoryUsedMB: number;
  globalMemoryTotalMB: number;
  globalMemoryUtilization: number;
  totalLoadCount: number;
  totalEvictionCount: number;
  totalThrashingEvents: number;
  averageLoadTimeMs: number;

  // Per-persona stats (all or single)
  personas: PersonaGenomeInfo[];
  error?: string;
}

/**
 * Helper to create GenomeStatsResult from params
 */
export const createGenomeStatsResultFromParams = (
  params: GenomeStatsParams,
  differences: Omit<Partial<GenomeStatsResult>, 'context' | 'sessionId'>
): GenomeStatsResult => transformPayload(params, {
  success: false,
  globalMemoryUsedMB: 0,
  globalMemoryTotalMB: 0,
  globalMemoryUtilization: 0,
  totalLoadCount: 0,
  totalEvictionCount: 0,
  totalThrashingEvents: 0,
  averageLoadTimeMs: 0,
  personas: [],
  ...differences
});
