/**
 * Genome Command Types
 *
 * Commands for managing LoRA adapter paging across personas.
 *
 * Phase 7: Single adapter per persona (mock adapters)
 * Phase 8+: Multiple adapters stacked (real GPU)
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult } from '../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../system/core/types/JTAGTypes';

// ========== genome/activate ==========

/**
 * Activate adapter for persona
 *
 * Loads adapter into memory, evicting others if needed.
 */
export interface GenomeActivateParams extends CommandParams {
  personaId: UUID;
  adapterId: UUID;
}

export interface GenomeActivateResult extends CommandResult {
  success: boolean;
  loaded: boolean;          // True if loaded, false if thrashing detected
  evictedAdapters?: UUID[]; // Adapters that were evicted
  loadTimeMs?: number;
  thrashingDetected?: boolean;
  error?: string;
}

/**
 * Helper to create GenomeActivateResult from params
 */
export const createGenomeActivateResultFromParams = (
  params: GenomeActivateParams,
  differences: Omit<Partial<GenomeActivateResult>, 'context' | 'sessionId'>
): GenomeActivateResult => transformPayload(params, {
  success: false,
  loaded: false,
  ...differences
});

// ========== genome/deactivate ==========

/**
 * Deactivate adapter for persona
 *
 * Unloads adapter from memory.
 */
export interface GenomeDeactivateParams extends CommandParams {
  personaId: UUID;
  adapterId: UUID;
}

export interface GenomeDeactivateResult extends CommandResult {
  success: boolean;
  unloaded: boolean;
  error?: string;
}

/**
 * Helper to create GenomeDeactivateResult from params
 */
export const createGenomeDeactivateResultFromParams = (
  params: GenomeDeactivateParams,
  differences: Omit<Partial<GenomeDeactivateResult>, 'context' | 'sessionId'>
): GenomeDeactivateResult => transformPayload(params, {
  success: false,
  unloaded: false,
  ...differences
});

// ========== genome/stats ==========

/**
 * Get genome statistics
 *
 * Returns global daemon stats and per-persona stats.
 */
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

// ========== genome/register ==========

/**
 * Register persona with genome daemon
 *
 * Must be called before activating adapters.
 */
export interface GenomeRegisterParams extends CommandParams {
  personaId: UUID;
  displayName: string;
  quotaMB?: number;
  priority?: number;
}

export interface GenomeRegisterResult extends CommandResult {
  success: boolean;
  registered: boolean;
  error?: string;
}

/**
 * Helper to create GenomeRegisterResult from params
 */
export const createGenomeRegisterResultFromParams = (
  params: GenomeRegisterParams,
  differences: Omit<Partial<GenomeRegisterResult>, 'context' | 'sessionId'>
): GenomeRegisterResult => transformPayload(params, {
  success: false,
  registered: false,
  ...differences
});

// ========== genome/unregister ==========

/**
 * Unregister persona from genome daemon
 *
 * Unloads all adapters for persona.
 */
export interface GenomeUnregisterParams extends CommandParams {
  personaId: UUID;
}

export interface GenomeUnregisterResult extends CommandResult {
  success: boolean;
  unregistered: boolean;
  error?: string;
}

/**
 * Helper to create GenomeUnregisterResult from params
 */
export const createGenomeUnregisterResultFromParams = (
  params: GenomeUnregisterParams,
  differences: Omit<Partial<GenomeUnregisterResult>, 'context' | 'sessionId'>
): GenomeUnregisterResult => transformPayload(params, {
  success: false,
  unregistered: false,
  ...differences
});
