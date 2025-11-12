/**
 * Genome Commands - Server Implementation
 *
 * Server-side handlers for genome commands.
 * Coordinates with GenomeDaemon.
 */

import type {
  GenomeActivateParams,
  GenomeActivateResult,
  GenomeDeactivateParams,
  GenomeDeactivateResult,
  GenomeStatsParams,
  GenomeStatsResult,
  GenomeRegisterParams,
  GenomeRegisterResult,
  GenomeUnregisterParams,
  GenomeUnregisterResult
} from '../shared/GenomeTypes';
import {
  createGenomeActivateResultFromParams,
  createGenomeDeactivateResultFromParams,
  createGenomeStatsResultFromParams,
  createGenomeRegisterResultFromParams,
  createGenomeUnregisterResultFromParams
} from '../shared/GenomeTypes';
import { GenomeDaemon } from '../../../system/genome/server/GenomeDaemon';

/**
 * Get genome daemon singleton
 */
function getDaemon(): GenomeDaemon {
  return GenomeDaemon.getInstance({
    totalMemoryMB: 8192,          // 8GB GPU memory (adjust for your hardware)
    defaultPersonaQuotaMB: 1024,  // 1GB per persona default
    hysteresisSeconds: 60,        // 60s thrashing protection
    enableThrashingProtection: true
  });
}

/**
 * genome/activate - Activate adapter for persona
 */
export async function genomeActivate(
  params: GenomeActivateParams
): Promise<GenomeActivateResult> {
  try {
    const daemon = getDaemon();

    const loaded = await daemon.loadAdapter(params.personaId, params.adapterId);

    if (!loaded) {
      return createGenomeActivateResultFromParams(params, {
        success: true,
        loaded: false,
        thrashingDetected: true
      });
    }

    // Get eviction stats (simplified - would need to track this in daemon)
    const stats = daemon.getStats();

    return createGenomeActivateResultFromParams(params, {
      success: true,
      loaded: true,
      thrashingDetected: false,
      loadTimeMs: stats.averageLoadTimeMs
    });
  } catch (error) {
    return createGenomeActivateResultFromParams(params, {
      success: false,
      loaded: false,
      error: error instanceof Error ? error.message : String(error)
    });
  }
}

/**
 * genome/deactivate - Deactivate adapter for persona
 */
export async function genomeDeactivate(
  params: GenomeDeactivateParams
): Promise<GenomeDeactivateResult> {
  try {
    const daemon = getDaemon();

    await daemon.unloadAdapter(params.personaId, params.adapterId);

    return createGenomeDeactivateResultFromParams(params, {
      success: true,
      unloaded: true
    });
  } catch (error) {
    return createGenomeDeactivateResultFromParams(params, {
      success: false,
      unloaded: false,
      error: error instanceof Error ? error.message : String(error)
    });
  }
}

/**
 * genome/stats - Get genome statistics
 */
export async function genomeStats(
  params: GenomeStatsParams
): Promise<GenomeStatsResult> {
  try {
    const daemon = getDaemon();
    const stats = daemon.getStats();
    const config = daemon.getConfig();

    // Get personas
    const allPersonas = daemon.listPersonas();
    const personasToShow = params.personaId
      ? allPersonas.filter(p => p.getPersonaId() === params.personaId)
      : allPersonas;

    const personas = personasToShow.map(state => ({
      personaId: state.getPersonaId(),
      displayName: state.getDisplayName(),
      memoryUsedMB: state.getMemoryUsed(),
      memoryQuotaMB: state.getMemoryQuota(),
      memoryUtilization: state.getMemoryUtilization(),
      activeAdapters: state.getActiveAdapters(),
      priority: state.getPriority(),
      loadCount: state.getLoadCount(),
      evictionCount: state.getEvictionCount(),
      lastActivatedAt: state.getLastActivatedAt()
    }));

    return createGenomeStatsResultFromParams(params, {
      success: true,
      globalMemoryUsedMB: stats.currentMemoryUsedMB,
      globalMemoryTotalMB: config.totalMemoryMB,
      globalMemoryUtilization: stats.currentMemoryUtilization,
      totalLoadCount: stats.totalLoadCount,
      totalEvictionCount: stats.totalEvictionCount,
      totalThrashingEvents: stats.totalThrashingEvents,
      averageLoadTimeMs: stats.averageLoadTimeMs,
      personas
    });
  } catch (error) {
    return createGenomeStatsResultFromParams(params, {
      success: false,
      error: error instanceof Error ? error.message : String(error)
    });
  }
}

/**
 * genome/register - Register persona with daemon
 */
export async function genomeRegister(
  params: GenomeRegisterParams
): Promise<GenomeRegisterResult> {
  try {
    const daemon = getDaemon();

    daemon.registerPersona(
      params.personaId,
      params.displayName,
      params.quotaMB,
      params.priority
    );

    return createGenomeRegisterResultFromParams(params, {
      success: true,
      registered: true
    });
  } catch (error) {
    return createGenomeRegisterResultFromParams(params, {
      success: false,
      registered: false,
      error: error instanceof Error ? error.message : String(error)
    });
  }
}

/**
 * genome/unregister - Unregister persona from daemon
 */
export async function genomeUnregister(
  params: GenomeUnregisterParams
): Promise<GenomeUnregisterResult> {
  try {
    const daemon = getDaemon();

    await daemon.unregisterPersona(params.personaId);

    return createGenomeUnregisterResultFromParams(params, {
      success: true,
      unregistered: true
    });
  } catch (error) {
    return createGenomeUnregisterResultFromParams(params, {
      success: false,
      unregistered: false,
      error: error instanceof Error ? error.message : String(error)
    });
  }
}
