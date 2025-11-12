/**
 * Genome Daemon - Global LoRA Adapter Coordination
 *
 * Separate process that manages adapter paging across all personas.
 * Prevents thrashing, enforces global memory limits, tracks statistics.
 *
 * Phase 7: Single adapter per persona, mock adapters only
 * Phase 8+: Multiple adapters stacked, real GPU integration
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { AdapterRegistry } from '../shared/AdapterRegistry';
import { PersonaGenomeState } from '../shared/PersonaGenomeState';
import { MockLoRAAdapter } from '../shared/MockLoRAAdapter';
import {
  selectAdaptersWithThrashingProtection
} from '../shared/LRUEviction';

/**
 * Global genome daemon configuration
 */
export interface GenomeDaemonConfig {
  totalMemoryMB: number;           // Total GPU memory available (e.g. 8GB)
  defaultPersonaQuotaMB: number;   // Default quota per persona (e.g. 1GB)
  hysteresisSeconds: number;       // Thrashing protection window (default 60s)
  enableThrashingProtection: boolean;  // Enable/disable hysteresis
}

/**
 * Global statistics for genome daemon
 */
export interface GenomeDaemonStats {
  totalLoadCount: number;
  totalEvictionCount: number;
  totalThrashingEvents: number;
  averageLoadTimeMs: number;
  currentMemoryUsedMB: number;
  currentMemoryUtilization: number;
}

/**
 * Genome Daemon - Global Adapter Coordinator
 *
 * Singleton daemon running in separate process.
 * Manages adapters across all personas with global view.
 */
export class GenomeDaemon {
  private static instance: GenomeDaemon | null = null;

  // Global registry
  private readonly registry: AdapterRegistry;

  // Per-persona state tracking
  private readonly personaStates: Map<UUID, PersonaGenomeState> = new Map();

  // Configuration
  private readonly config: GenomeDaemonConfig;

  // Global statistics
  private stats: GenomeDaemonStats = {
    totalLoadCount: 0,
    totalEvictionCount: 0,
    totalThrashingEvents: 0,
    averageLoadTimeMs: 0,
    currentMemoryUsedMB: 0,
    currentMemoryUtilization: 0
  };

  private constructor(config: GenomeDaemonConfig) {
    this.config = config;
    this.registry = new AdapterRegistry();
  }

  /**
   * Get singleton instance
   */
  static getInstance(config?: GenomeDaemonConfig): GenomeDaemon {
    if (!GenomeDaemon.instance) {
      if (!config) {
        throw new Error('GenomeDaemon must be initialized with config on first call');
      }
      GenomeDaemon.instance = new GenomeDaemon(config);
    }
    return GenomeDaemon.instance;
  }

  /**
   * Reset singleton (for testing)
   */
  static resetInstance(): void {
    GenomeDaemon.instance = null;
  }

  // ========== Persona Management ==========

  /**
   * Register new persona with the daemon
   *
   * @param personaId UUID of persona
   * @param displayName Display name for logging
   * @param quotaMB Memory quota (defaults to config default)
   * @param priority Priority 0-1 (defaults to 0.5)
   */
  registerPersona(
    personaId: UUID,
    displayName: string,
    quotaMB?: number,
    priority?: number
  ): void {
    if (this.personaStates.has(personaId)) {
      throw new Error(`Persona ${personaId} is already registered`);
    }

    const state = new PersonaGenomeState({
      personaId,
      displayName,
      memoryQuotaMB: quotaMB || this.config.defaultPersonaQuotaMB,
      priority
    });

    this.personaStates.set(personaId, state);
  }

  /**
   * Unregister persona (cleanup)
   */
  async unregisterPersona(personaId: UUID): Promise<void> {
    const state = this.personaStates.get(personaId);
    if (!state) {
      throw new Error(`Persona ${personaId} is not registered`);
    }

    // Unload all adapters for this persona
    const activeAdapters = state.getActiveAdapters();
    for (const adapterId of activeAdapters) {
      const adapter = this.registry.getById(adapterId);
      if (adapter) {
        await this.unloadAdapter(personaId, adapterId);
      }
    }

    this.personaStates.delete(personaId);
  }

  /**
   * Get persona state
   */
  getPersonaState(personaId: UUID): PersonaGenomeState {
    const state = this.personaStates.get(personaId);
    if (!state) {
      throw new Error(`Persona ${personaId} is not registered`);
    }
    return state;
  }

  /**
   * List all registered personas
   */
  listPersonas(): PersonaGenomeState[] {
    return Array.from(this.personaStates.values());
  }

  // ========== Adapter Management ==========

  /**
   * Load adapter for specific persona
   *
   * This is the main entry point for genome activation.
   * Handles eviction if needed.
   *
   * @param personaId Persona requesting adapter
   * @param adapterId Adapter to load
   * @returns True if loaded, false if thrashing detected
   */
  async loadAdapter(personaId: UUID, adapterId: UUID): Promise<boolean> {
    const state = this.getPersonaState(personaId);
    const adapter = this.registry.getById(adapterId);

    if (!adapter) {
      throw new Error(`Adapter ${adapterId} not found in registry`);
    }

    // Already loaded?
    if (state.hasActiveAdapter(adapterId)) {
      adapter.markUsed();
      return true;
    }

    // Check if we need to evict
    const adapterSize = adapter.getSize();
    if (!this.canLoadAdapter(adapterSize)) {
      const evicted = await this.evictAdapters(adapterSize);
      if (!evicted) {
        this.stats.totalThrashingEvents++;
        return false;  // Would cause thrashing
      }
    }

    // Load adapter
    const loadStartTime = Date.now();
    await adapter.load();
    const loadTimeMs = Date.now() - loadStartTime;

    // Update state
    state.recordLoad(adapterId, adapterSize, loadTimeMs);

    // Update global stats
    this.stats.totalLoadCount++;
    this.stats.currentMemoryUsedMB += adapterSize;
    this.updateMemoryUtilization();
    this.updateAverageLoadTime(loadTimeMs);

    return true;
  }

  /**
   * Unload adapter for specific persona
   *
   * @param personaId Persona owning adapter
   * @param adapterId Adapter to unload
   */
  async unloadAdapter(personaId: UUID, adapterId: UUID): Promise<void> {
    const state = this.getPersonaState(personaId);
    const adapter = this.registry.getById(adapterId);

    if (!adapter) {
      throw new Error(`Adapter ${adapterId} not found in registry`);
    }

    if (!state.hasActiveAdapter(adapterId)) {
      throw new Error(`Adapter ${adapterId} is not loaded for persona ${personaId}`);
    }

    // Unload adapter
    await adapter.unload();

    // Update state
    const adapterSize = adapter.getSize();
    state.recordUnload(adapterId, adapterSize);

    // Update global stats
    this.stats.totalEvictionCount++;
    this.stats.currentMemoryUsedMB -= adapterSize;
    this.updateMemoryUtilization();
  }

  // ========== Memory Management ==========

  /**
   * Check if adapter can be loaded without eviction
   */
  canLoadAdapter(sizeMB: number): boolean {
    const available = this.config.totalMemoryMB - this.stats.currentMemoryUsedMB;
    return available >= sizeMB;
  }

  /**
   * Get current memory usage
   */
  getMemoryUsed(): number {
    return this.stats.currentMemoryUsedMB;
  }

  /**
   * Get available memory
   */
  getMemoryAvailable(): number {
    return Math.max(0, this.config.totalMemoryMB - this.stats.currentMemoryUsedMB);
  }

  /**
   * Get memory utilization (0.0-1.0)
   */
  getMemoryUtilization(): number {
    return this.stats.currentMemoryUtilization;
  }

  // ========== Eviction ==========

  /**
   * Evict adapters to free up target memory
   *
   * Uses LRU algorithm with thrashing protection.
   *
   * @param targetFreeMB How much memory we need
   * @returns True if target freed, false if would thrash
   */
  private async evictAdapters(targetFreeMB: number): Promise<boolean> {
    // Collect all loaded adapters across all personas
    const allLoadedAdapters: MockLoRAAdapter[] = [];

    for (const state of this.personaStates.values()) {
      const activeAdapters = state.getActiveAdapters();
      for (const adapterId of activeAdapters) {
        const adapter = this.registry.getById(adapterId);
        if (adapter) {
          allLoadedAdapters.push(adapter);
        }
      }
    }

    // Select adapters to evict with thrashing protection
    const hysteresis = this.config.enableThrashingProtection
      ? this.config.hysteresisSeconds
      : 0;

    const result = selectAdaptersWithThrashingProtection(
      allLoadedAdapters,
      targetFreeMB,
      hysteresis
    );

    if (result.wouldThrash) {
      // Can't free enough memory without thrashing
      return false;
    }

    // Evict selected adapters
    for (const adapter of result.toEvict) {
      // Find which persona owns this adapter
      const personaId = this.findAdapterOwner(adapter.getId());
      if (personaId) {
        await this.unloadAdapter(personaId, adapter.getId());
      }
    }

    return true;
  }

  /**
   * Find which persona owns an adapter
   */
  private findAdapterOwner(adapterId: UUID): UUID | null {
    for (const [personaId, state] of this.personaStates.entries()) {
      if (state.hasActiveAdapter(adapterId)) {
        return personaId;
      }
    }
    return null;
  }

  // ========== Statistics ==========

  /**
   * Get global daemon statistics
   */
  getStats(): Readonly<GenomeDaemonStats> {
    return { ...this.stats };
  }

  /**
   * Update memory utilization metric
   */
  private updateMemoryUtilization(): void {
    this.stats.currentMemoryUtilization =
      this.stats.currentMemoryUsedMB / this.config.totalMemoryMB;
  }

  /**
   * Update rolling average load time
   */
  private updateAverageLoadTime(newLoadTimeMs: number): void {
    const totalLoads = this.stats.totalLoadCount;
    if (totalLoads === 1) {
      this.stats.averageLoadTimeMs = newLoadTimeMs;
    } else {
      // Rolling average
      const prevAvg = this.stats.averageLoadTimeMs;
      this.stats.averageLoadTimeMs =
        (prevAvg * (totalLoads - 1) + newLoadTimeMs) / totalLoads;
    }
  }

  // ========== Registry Access ==========

  /**
   * Get adapter registry (for tests)
   */
  getRegistry(): AdapterRegistry {
    return this.registry;
  }

  /**
   * Get configuration
   */
  getConfig(): Readonly<GenomeDaemonConfig> {
    return { ...this.config };
  }
}
