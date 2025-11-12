/**
 * Persona Genome State - Per-Persona Memory Tracking
 *
 * Tracks the current genome state for ONE persona.
 * Pure data class - no I/O, no side effects.
 *
 * Phase 7: Single adapter per persona
 * Phase 8+: Multiple adapters stacked (genome)
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';

/**
 * Configuration for PersonaGenomeState initialization
 */
export interface PersonaGenomeConfig {
  personaId: UUID;
  displayName: string;
  memoryQuotaMB: number;  // From ResourceManager
  priority?: number;       // 0.0-1.0, default 0.5
}

/**
 * Statistics about genome usage
 */
export interface GenomeStats {
  loadCount: number;
  lastActivatedAt: number | null;
  totalLoadTimeMs: number;
  totalEvictionCount: number;
}

/**
 * Current state of a persona's genome
 */
export class PersonaGenomeState {
  // Identity
  private readonly personaId: UUID;
  private readonly displayName: string;

  // Memory tracking
  private memoryUsedMB: number = 0;
  private readonly memoryQuotaMB: number;

  // Current adapters (ONE in Phase 7, multiple in Phase 8+)
  private activeAdapters: Set<UUID> = new Set();

  // Statistics
  private stats: GenomeStats = {
    loadCount: 0,
    lastActivatedAt: null,
    totalLoadTimeMs: 0,
    totalEvictionCount: 0
  };

  // Priority (affects eviction decisions globally)
  private priority: number;

  constructor(config: PersonaGenomeConfig) {
    this.personaId = config.personaId;
    this.displayName = config.displayName;
    this.memoryQuotaMB = config.memoryQuotaMB;
    this.priority = config.priority ?? 0.5;

    // Validate priority range
    if (this.priority < 0 || this.priority > 1) {
      throw new Error(`Priority must be between 0 and 1, got ${this.priority}`);
    }

    // Validate quota
    if (this.memoryQuotaMB <= 0) {
      throw new Error(`Memory quota must be positive, got ${this.memoryQuotaMB}MB`);
    }
  }

  // ========== Identity Accessors ==========

  getPersonaId(): UUID {
    return this.personaId;
  }

  getDisplayName(): string {
    return this.displayName;
  }

  getPriority(): number {
    return this.priority;
  }

  setPriority(priority: number): void {
    if (priority < 0 || priority > 1) {
      throw new Error(`Priority must be between 0 and 1, got ${priority}`);
    }
    this.priority = priority;
  }

  // ========== Memory Tracking ==========

  getMemoryUsed(): number {
    return this.memoryUsedMB;
  }

  getMemoryQuota(): number {
    return this.memoryQuotaMB;
  }

  getMemoryAvailable(): number {
    return Math.max(0, this.memoryQuotaMB - this.memoryUsedMB);
  }

  getMemoryUtilization(): number {
    if (this.memoryQuotaMB === 0) return 0;
    return this.memoryUsedMB / this.memoryQuotaMB;
  }

  isOverQuota(): boolean {
    return this.memoryUsedMB > this.memoryQuotaMB;
  }

  canLoadAdapter(sizeMB: number): boolean {
    return this.getMemoryAvailable() >= sizeMB;
  }

  // ========== Adapter Management ==========

  hasActiveAdapter(adapterId: UUID): boolean {
    return this.activeAdapters.has(adapterId);
  }

  getActiveAdapters(): UUID[] {
    return Array.from(this.activeAdapters);
  }

  getActiveAdapterCount(): number {
    return this.activeAdapters.size;
  }

  /**
   * Record adapter load
   *
   * @param adapterId Adapter being loaded
   * @param sizeMB Memory consumed by adapter
   * @param loadTimeMs Time taken to load
   */
  recordLoad(adapterId: UUID, sizeMB: number, loadTimeMs: number): void {
    if (this.activeAdapters.has(adapterId)) {
      throw new Error(`Adapter ${adapterId} is already loaded`);
    }

    this.activeAdapters.add(adapterId);
    this.memoryUsedMB += sizeMB;
    this.stats.loadCount++;
    this.stats.lastActivatedAt = Date.now();
    this.stats.totalLoadTimeMs += loadTimeMs;
  }

  /**
   * Record adapter unload/eviction
   *
   * @param adapterId Adapter being unloaded
   * @param sizeMB Memory freed by unload
   */
  recordUnload(adapterId: UUID, sizeMB: number): void {
    if (!this.activeAdapters.has(adapterId)) {
      throw new Error(`Adapter ${adapterId} is not loaded`);
    }

    this.activeAdapters.delete(adapterId);
    this.memoryUsedMB -= sizeMB;
    this.stats.totalEvictionCount++;

    // Ensure memory doesn't go negative (floating point errors)
    if (this.memoryUsedMB < 0.01) {
      this.memoryUsedMB = 0;
    }
  }

  // ========== Statistics ==========

  getStats(): Readonly<GenomeStats> {
    return { ...this.stats };
  }

  getLoadCount(): number {
    return this.stats.loadCount;
  }

  getLastActivatedAt(): number | null {
    return this.stats.lastActivatedAt;
  }

  getTotalLoadTime(): number {
    return this.stats.totalLoadTimeMs;
  }

  getAverageLoadTime(): number {
    if (this.stats.loadCount === 0) return 0;
    return this.stats.totalLoadTimeMs / this.stats.loadCount;
  }

  getEvictionCount(): number {
    return this.stats.totalEvictionCount;
  }

  /**
   * Calculate eviction score for this persona
   *
   * Higher score = more likely to evict this persona's adapters.
   * Considers:
   * - Age (how long since last activation)
   * - Priority (high priority = low eviction score)
   * - Memory pressure (over quota = higher score)
   */
  calculateEvictionScore(): number {
    // High-priority personas never evict
    if (this.priority > 0.9) {
      return -Infinity;
    }

    // Calculate age (seconds since last activation)
    const now = Date.now();
    const lastActivated = this.stats.lastActivatedAt ?? now;
    const ageSeconds = (now - lastActivated) / 1000;

    // Priority weight (lower priority = easier to evict)
    const priorityWeight = Math.max(this.priority, 0.1);

    // Memory pressure bonus (over quota = easier to evict)
    const pressureMultiplier = this.isOverQuota() ? 2.0 : 1.0;

    // Final score: age / priority (higher = evict first)
    return (ageSeconds / (priorityWeight * 10)) * pressureMultiplier;
  }

  // ========== Serialization ==========

  /**
   * Export state to JSON (for persistence)
   */
  toJSON(): Record<string, unknown> {
    return {
      personaId: this.personaId,
      displayName: this.displayName,
      memoryUsedMB: this.memoryUsedMB,
      memoryQuotaMB: this.memoryQuotaMB,
      activeAdapters: Array.from(this.activeAdapters),
      priority: this.priority,
      stats: this.stats
    };
  }

  /**
   * Create from JSON (for restoration)
   */
  static fromJSON(data: Record<string, unknown>): PersonaGenomeState {
    const state = new PersonaGenomeState({
      personaId: data.personaId as UUID,
      displayName: data.displayName as string,
      memoryQuotaMB: data.memoryQuotaMB as number,
      priority: data.priority as number
    });

    // Restore memory tracking
    state.memoryUsedMB = data.memoryUsedMB as number;

    // Restore active adapters
    const adapters = data.activeAdapters as string[];
    adapters.forEach(id => state.activeAdapters.add(id as UUID));

    // Restore stats
    const stats = data.stats as GenomeStats;
    state.stats = { ...stats };

    return state;
  }
}
