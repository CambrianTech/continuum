/**
 * SystemSchedulingState - Living Hive Mind Scheduler
 *
 * Singleton that provides adaptive cadence recommendations to all BaseSleepingEntity instances.
 *
 * Responsibilities:
 * - Load SystemSchedulingConfigEntity from database on startup
 * - Subscribe to config updates (humans/AIs can modify via commands)
 * - Calculate recommended cadences based on:
 *   1. Base timing (from config)
 *   2. Manual adjustments (from config)
 *   3. AI count scaling (sqrt by default)
 *   4. Load scaling (exponential under stress)
 *
 * Integration:
 * - BaseSleepingEntity calls `getRecommendedCadence(entityType)` in `getSleepDuration()`
 * - Commands modify config entity → Events update this singleton → All entities adapt
 *
 * Example Calculation:
 * - Base timing: 30s (adapter-health-check)
 * - Manual adjustment: 1.5x (user slowed it down)
 * - AI count: 13 AIs → sqrt(13) = 3.6x scaling
 * - Load: 0.8 (80%) → (0.8^4) = 0.41 → 1/(0.41) = 2.4x slowdown
 * - Final: 30s * 1.5 * 3.6 * 2.4 = 388s (~6.5 min)
 *
 * Philosophy: Graceful degradation under stress
 * - Light load + few AIs = fast cadences (responsive)
 * - Heavy load + many AIs = slow cadences (sustainable)
 * - System "breathes" naturally based on demand
 */

import { Commands } from '../../core/shared/Commands';
import { Events } from '../../core/shared/Events';
import { SystemConfigEntity } from '../../data/entities/SystemConfigEntity';
import { DATA_COMMANDS } from '../../../commands/data/shared/DataCommandConstants';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import type { DataCreateParams, DataCreateResult } from '../../../commands/data/create/shared/DataCreateTypes';
import type { DataUpdateParams, DataUpdateResult } from '../../../commands/data/update/shared/DataUpdateTypes';
import type { BaseEntity } from '../../data/entities/BaseEntity';

import { DataList } from '../../../commands/data/list/shared/DataListTypes';
import { DataCreate } from '../../../commands/data/create/shared/DataCreateTypes';
import { DataUpdate } from '../../../commands/data/update/shared/DataUpdateTypes';
export type AICountScalingPolicy = 'none' | 'linear' | 'sqrt' | 'log';

export class SystemSchedulingState {
  private static _instance: SystemSchedulingState | null = null;
  private _config: SystemConfigEntity | null = null;
  private _initialized: boolean = false;

  private constructor() {
    // Private constructor for singleton
  }

  /**
   * Get singleton instance
   */
  static get instance(): SystemSchedulingState {
    if (!SystemSchedulingState._instance) {
      SystemSchedulingState._instance = new SystemSchedulingState();
    }
    return SystemSchedulingState._instance;
  }

  /**
   * Initialize system - load config from database
   * MUST be called on system startup before any BaseSleepingEntity is created
   */
  async initialize(): Promise<void> {
    if (this._initialized) {
      return; // Already initialized
    }

    try {
      // Query for singleton config entity
      const result = await DataList.execute({
        collection: SystemConfigEntity.collection,
        filter: { name: 'default' },
        limit: 1,
      });

      if (result.items.length === 0) {
        // First startup - create default config
        console.log('[SystemSchedulingState] No config found - creating default');
        this._config = await this.createDefaultConfig();
      } else {
        // Load existing config
        this._config = result.items[0] as SystemConfigEntity;
        console.log('[SystemSchedulingState] Loaded config:', this._config.id);
      }

      // Subscribe to config updates
      Events.subscribe('data:system_config:updated', this.handleConfigUpdate.bind(this));
      Events.subscribe('data:system_config:created', this.handleConfigUpdate.bind(this));

      this._initialized = true;
    } catch (error) {
      console.error('[SystemSchedulingState] Initialization failed:', error);
      throw error;
    }
  }

  /**
   * Create default configuration entity
   */
  private async createDefaultConfig(): Promise<SystemSchedulingConfigEntity> {
    const config = new SystemSchedulingConfigEntity();

    // Use data/create command to persist
    const result = await DataCreate.execute({
      collection: SystemSchedulingConfigEntity.collection,
      entity: config,
    });

    return result.entity as SystemSchedulingConfigEntity;
  }

  /**
   * Handle config update events
   */
  private handleConfigUpdate(event: any): void {
    if (event.entity && event.entity.name === 'default') {
      console.log('[SystemSchedulingState] Config updated:', event.entity.id);
      this._config = event.entity as SystemSchedulingConfigEntity;
    }
  }

  /**
   * Get current configuration (read-only)
   */
  get config(): SystemSchedulingConfigEntity {
    if (!this._initialized || !this._config) {
      throw new Error('SystemSchedulingState not initialized - call initialize() first');
    }
    return this._config;
  }

  /**
   * Get recommended cadence for an entity type
   *
   * This is the MAIN API used by BaseSleepingEntity implementations.
   *
   * @param entityType - Type of entity requesting cadence (e.g., 'persona-inbox', 'adapter-health-check')
   * @returns Recommended sleep duration in milliseconds
   */
  getRecommendedCadence(entityType: string): number {
    if (!this._initialized || !this._config) {
      // Fallback to conservative default if not initialized
      console.warn(`[SystemSchedulingState] Not initialized - using fallback for '${entityType}'`);
      return 10000; // 10s default
    }

    try {
      // 1. Get base cadence (includes manual adjustments)
      const baseCadence = this._config.getBaseCadence(entityType);

      // 2. Apply AI count scaling
      let cadence = baseCadence;
      if (this._config.scalingPolicy.aiCountScaling !== 'none') {
        const aiScale = this.calculateAIScaling(
          this._config.systemState.activeAICount,
          this._config.scalingPolicy.aiCountScaling
        );
        cadence *= aiScale;
      }

      // 3. Apply load scaling (if enabled and above threshold)
      if (this._config.scalingPolicy.loadScalingEnabled) {
        const currentLoad = this._config.systemState.currentLoad;
        if (currentLoad > this._config.scalingPolicy.loadScalingThreshold) {
          const loadScale = this.calculateLoadScaling(
            currentLoad,
            this._config.scalingPolicy.loadScalingExponent
          );
          cadence *= loadScale;
        }
      }

      return Math.round(cadence);
    } catch (error) {
      console.error(`[SystemSchedulingState] Error calculating cadence for '${entityType}':`, error);
      return 10000; // Safe fallback
    }
  }

  /**
   * Calculate AI count scaling factor
   *
   * @param aiCount - Number of active AIs
   * @param policy - Scaling policy ('none', 'linear', 'sqrt', 'log')
   * @returns Scaling multiplier (1.0 = no scaling)
   */
  private calculateAIScaling(aiCount: number, policy: AICountScalingPolicy): number {
    if (aiCount <= 1) {
      return 1.0; // No scaling for 1 AI
    }

    switch (policy) {
      case 'none':
        return 1.0;

      case 'linear':
        // Linear scaling: 2 AIs = 2x slower, 13 AIs = 13x slower
        return aiCount;

      case 'sqrt':
        // Square root scaling: 4 AIs = 2x, 9 AIs = 3x, 13 AIs = 3.6x
        // Good balance - sub-linear but still noticeable
        return Math.sqrt(aiCount);

      case 'log':
        // Logarithmic scaling: very gentle
        // 10 AIs = 2.3x, 100 AIs = 4.6x
        return Math.log10(aiCount * 10);

      default:
        console.warn(`[SystemSchedulingState] Unknown AI scaling policy '${policy}' - using sqrt`);
        return Math.sqrt(aiCount);
    }
  }

  /**
   * Calculate load scaling factor
   *
   * Uses exponential scaling - system slows down aggressively under high load.
   *
   * @param load - Current load (0.0-1.0)
   * @param exponent - Exponential factor (default 4)
   * @returns Scaling multiplier (>= 1.0)
   */
  private calculateLoadScaling(load: number, exponent: number): number {
    if (load <= 0) {
      return 1.0; // No load
    }

    // Inverse exponential: higher load = slower cadence
    // Example with exponent=4:
    // - 50% load: (0.5^4) = 0.0625 → 1/0.0625 = 16x slower
    // - 80% load: (0.8^4) = 0.41 → 1/0.41 = 2.4x slower
    // - 90% load: (0.9^4) = 0.66 → 1/0.66 = 1.5x slower
    // - 100% load: (1.0^4) = 1.0 → 1.0x (no further slowdown)
    //
    // Wait, that's backwards. Let me fix:
    // We want SLOWER when load is HIGH, so:
    // - Lower load value should give LOWER scaling
    // - Higher load value should give HIGHER scaling
    //
    // Correct formula: 1 / (1 - load)^exponent
    // But this explodes at load=1.0, so cap it:
    const normalizedLoad = Math.min(load, 0.99); // Cap at 99%
    const factor = Math.pow(1 - normalizedLoad, exponent);

    // Inverse to get slowdown multiplier
    // At 50% load: (1-0.5)^4 = 0.0625 → 1/0.0625 = 16x slower
    // At 80% load: (1-0.8)^4 = 0.0016 → 1/0.0016 = 625x slower (WAY too aggressive)
    //
    // Actually, let's use simpler formula:
    // multiplier = 1 + (load ^ exponent) * maxSlowdown
    //
    // Where maxSlowdown = 10 (at 100% load, 10x slower)
    const maxSlowdown = 10;
    return 1 + Math.pow(normalizedLoad, exponent) * maxSlowdown;
  }

  /**
   * Update system load state
   * Called periodically by system monitors to reflect current CPU/memory usage
   *
   * @param load - Current load (0.0-1.0)
   * @param activeAICount - Number of active AI personas
   */
  async updateSystemLoad(load: number, activeAICount: number): Promise<void> {
    if (!this._initialized || !this._config) {
      console.warn('[SystemSchedulingState] Cannot update load - not initialized');
      return;
    }

    // Update config entity
    this._config.systemState = {
      currentLoad: Math.max(0, Math.min(1, load)), // Clamp to 0-1
      activeAICount: Math.max(0, activeAICount),
      lastUpdated: Date.now(),
    };

    // Persist to database
    try {
      await DataUpdate.execute({
        collection: SystemSchedulingConfigEntity.collection,
        id: this._config.id,
        updates: {
          systemState: this._config.systemState,
        },
      });
    } catch (error) {
      console.error('[SystemSchedulingState] Failed to update system load:', error);
    }
  }

  /**
   * Check if initialized
   */
  get initialized(): boolean {
    return this._initialized;
  }

  /**
   * Get current system load (0.0-1.0)
   */
  get currentLoad(): number {
    return this._config?.systemState.currentLoad ?? 0;
  }

  /**
   * Get active AI count
   */
  get activeAICount(): number {
    return this._config?.systemState.activeAICount ?? 0;
  }
}

/**
 * Convenience export - global singleton instance
 */
export const schedulingState = SystemSchedulingState.instance;
