/**
 * FitnessTracker - Debounced fitness recording for genome layers
 *
 * Accumulates inference results in memory and persists to the database
 * every 30 seconds. This prevents per-inference DB writes while still
 * maintaining accurate fitness metrics for natural selection.
 *
 * The updateFitness() method on GenomeLayerEntity does the actual math
 * (EMA for success rate, running average for latency, etc.). This class
 * just batches the calls and persists.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { GenomeLayerEntity } from '../entities/GenomeLayerEntity';
import { ORM } from '../../../daemons/data-daemon/server/ORM';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('FitnessTracker', 'genome');

interface PendingUpdate {
  successCount: number;
  totalCount: number;
  totalLatency: number;
  cacheHits: number;
}

const PERSIST_INTERVAL_MS = 30_000;

export class FitnessTracker {
  private static _instance: FitnessTracker;

  private pending: Map<string, PendingUpdate> = new Map();
  private persistTimer: NodeJS.Timeout | null = null;

  static get instance(): FitnessTracker {
    if (!FitnessTracker._instance) {
      FitnessTracker._instance = new FitnessTracker();
    }
    return FitnessTracker._instance;
  }

  /**
   * Record a single inference result for a genome layer.
   * Accumulates in memory; persisted on the next 30s flush.
   */
  recordInference(layerId: UUID | string, result: { success: boolean; latency: number; cacheHit?: boolean }): void {
    const existing = this.pending.get(layerId) ?? { successCount: 0, totalCount: 0, totalLatency: 0, cacheHits: 0 };

    existing.totalCount++;
    existing.totalLatency += result.latency;
    if (result.success) existing.successCount++;
    if (result.cacheHit) existing.cacheHits++;

    this.pending.set(layerId, existing);

    // Start persist timer on first entry
    if (!this.persistTimer) {
      this.persistTimer = setTimeout(() => this.persistAll(), PERSIST_INTERVAL_MS);
    }
  }

  /**
   * Flush all pending updates to the database immediately.
   * Called on shutdown or when immediate persistence is needed.
   */
  async flush(): Promise<void> {
    if (this.persistTimer) {
      clearTimeout(this.persistTimer);
      this.persistTimer = null;
    }
    await this.persistAll();
  }

  private async persistAll(): Promise<void> {
    this.persistTimer = null;

    if (this.pending.size === 0) return;

    // Snapshot and clear pending (so new recordings don't interfere)
    const snapshot = new Map(this.pending);
    this.pending.clear();

    let persisted = 0;
    for (const [layerId, update] of snapshot) {
      try {
        // Load entity from database
        const entity = await ORM.read<GenomeLayerEntity>(
          GenomeLayerEntity.collection,
          layerId,
          'default',
        );

        if (!entity) {
          log.warn(`Layer ${layerId} not found in database, dropping ${update.totalCount} fitness updates`);
          continue;
        }

        // Apply each inference result to the entity's fitness (EMA math lives on the entity)
        for (let i = 0; i < update.totalCount; i++) {
          const wasSuccess = i < update.successCount;
          const avgLatency = update.totalLatency / update.totalCount;
          const wasCacheHit = i < update.cacheHits;
          entity.updateFitness({ success: wasSuccess, latency: avgLatency, cacheHit: wasCacheHit });
        }

        // Persist updated fitness
        await ORM.update<GenomeLayerEntity>(
          GenomeLayerEntity.collection,
          layerId,
          { fitness: entity.fitness },
          false,
          'default',
        );

        persisted++;
      } catch (error) {
        log.error(`Failed to persist fitness for layer ${layerId}: ${error}`);
        // Re-enqueue failed updates so they're not lost
        const existing = this.pending.get(layerId);
        if (existing) {
          existing.totalCount += update.totalCount;
          existing.successCount += update.successCount;
          existing.totalLatency += update.totalLatency;
          existing.cacheHits += update.cacheHits;
        } else {
          this.pending.set(layerId, update);
        }
      }
    }

    if (persisted > 0) {
      log.info(`Persisted fitness updates for ${persisted} layers`);
    }
  }
}
