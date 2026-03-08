/**
 * MetricsCollector - Fire-and-forget system metrics persistence
 *
 * Subscribes to existing ResourcePressureWatcher and GpuPressureWatcher events.
 * Writes snapshots to a dedicated metrics database via ORM.
 * Never blocks the main system — all writes are fire-and-forget.
 *
 * Sampling: Batches incoming pressure events and writes one snapshot every
 * SAMPLE_INTERVAL_MS. This prevents flooding the database when watchers
 * poll at 500ms-1s under high pressure.
 *
 * Auto-prunes data older than RETENTION_DAYS to prevent unbounded growth.
 */

import { Events } from '@system/core/shared/Events';
import { SystemPaths } from '@system/core/config/SystemPaths';
import { ORM } from '../../../daemons/data-daemon/server/ORM';
import { DatabaseHandleRegistry, type DbHandle } from '../../../daemons/data-daemon/server/DatabaseHandleRegistry';
import { SystemMetricsEntity } from '@system/data/entities/SystemMetricsEntity';
import type { CollectionName } from '../../../shared/generated-collection-constants';
import * as fs from 'fs';
import * as path from 'path';

// ── Constants ────────────────────────────────────────────────────────────────

/** How often to write a sample to the database (30s = 2880 samples/day) */
const SAMPLE_INTERVAL_MS = 30_000;

/** How many days of data to retain */
const RETENTION_DAYS = 30;

/** Prune check interval (every 6 hours) */
const PRUNE_INTERVAL_MS = 6 * 60 * 60 * 1000;

// ── MetricsCollector ─────────────────────────────────────────────────────────

export class MetricsCollector {
  private static _instance: MetricsCollector | null = null;

  private _running = false;
  private _handle: DbHandle | null = null;
  private _sampleTimer: ReturnType<typeof setInterval> | null = null;
  private _pruneTimer: ReturnType<typeof setInterval> | null = null;
  private _unsubs: Array<() => void> = [];

  // Latest values from watchers (updated on every event, sampled periodically)
  private _cpuUsage = 0;
  private _cpuCores = 0;
  private _memoryPressure = 0;
  private _memoryTotalMb = 0;
  private _memoryUsedMb = 0;
  private _gpuPressure = 0;
  private _gpuTotalMb = 0;
  private _gpuUsedMb = 0;
  private _gpuName: string | undefined;
  private _hasReceivedData = false;

  static get instance(): MetricsCollector {
    if (!MetricsCollector._instance) {
      MetricsCollector._instance = new MetricsCollector();
    }
    return MetricsCollector._instance;
  }

  get running(): boolean {
    return this._running;
  }

  /** Start collecting metrics. Call after DataDaemon and pressure watchers are initialized. */
  async start(): Promise<void> {
    if (this._running) return;
    this._running = true;

    try {
      // Ensure metrics directory exists
      const metricsDir = SystemPaths.metrics.root;
      if (!fs.existsSync(metricsDir)) {
        fs.mkdirSync(metricsDir, { recursive: true });
      }

      // Open dedicated database handle for metrics
      const registry = DatabaseHandleRegistry.getInstance();
      this._handle = await registry.open('sqlite', {
        path: SystemPaths.metrics.database,
      }, { emitEvents: false }); // No CRUD events for metrics — pure telemetry

      registry.registerAlias('metrics', this._handle);

      // Subscribe to pressure watcher events
      this._unsubs.push(
        Events.subscribe('cpu:pressure:update', (update: any) => {
          this._cpuUsage = update.pressure ?? 0;
          this._hasReceivedData = true;
        }),
        Events.subscribe('memory:pressure:update', (update: any) => {
          this._memoryPressure = update.pressure ?? 0;
          this._hasReceivedData = true;
        }),
        Events.subscribe('gpu:pressure:update', (update: any) => {
          this._gpuPressure = update.pressure ?? 0;
          this._hasReceivedData = true;
        })
      );

      // Enrich with full stats from watchers (lazy — gets richer data when available)
      this._enrichFromWatchers();

      // Periodic sampling — write snapshot every SAMPLE_INTERVAL_MS
      this._sampleTimer = setInterval(() => this._writeSample(), SAMPLE_INTERVAL_MS);

      // Write first sample after a short delay (let watchers emit initial values)
      setTimeout(() => this._writeSample(), 5_000);

      // Periodic pruning — clean old data every PRUNE_INTERVAL_MS
      this._pruneTimer = setInterval(() => this._pruneOldData(), PRUNE_INTERVAL_MS);

      // Initial prune on startup
      this._pruneOldData();

      console.log(`[MetricsCollector] Started — sampling every ${SAMPLE_INTERVAL_MS / 1000}s, retaining ${RETENTION_DAYS} days`);
    } catch (error) {
      console.error('[MetricsCollector] Failed to start:', error);
      this._running = false;
    }
  }

  /** Stop collecting. */
  stop(): void {
    this._running = false;
    this._unsubs.forEach(u => u());
    this._unsubs = [];
    if (this._sampleTimer) {
      clearInterval(this._sampleTimer);
      this._sampleTimer = null;
    }
    if (this._pruneTimer) {
      clearInterval(this._pruneTimer);
      this._pruneTimer = null;
    }
    console.log('[MetricsCollector] Stopped');
  }

  /** Get the metrics database handle for queries. */
  get handle(): DbHandle | null {
    return this._handle;
  }

  // ── Internal ──────────────────────────────────────────────────────

  /** Pull enriched data from watcher singletons (if available). */
  private _enrichFromWatchers(): void {
    try {
      // Dynamic import to avoid circular deps — watchers may not be started yet
      const { ResourcePressureWatcher } = require('../../resources/server/ResourcePressureWatcher');
      const watcher = ResourcePressureWatcher.instance;
      if (watcher.cpu) {
        this._cpuCores = watcher.cpu.physicalCores ?? 0;
      }
      if (watcher.memory) {
        this._memoryTotalMb = Math.round((watcher.memory.totalBytes ?? 0) / (1024 * 1024));
        this._memoryUsedMb = Math.round((watcher.memory.usedBytes ?? 0) / (1024 * 1024));
      }
    } catch { /* watchers not ready yet, will get data from events */ }

    try {
      const { GpuPressureWatcher } = require('../../gpu/server/GpuPressureWatcher');
      // GPU pressure watcher only caches pressure, not full stats
      // Full GPU stats come from periodic enrichment below
    } catch { /* ok */ }
  }

  /** Enrich GPU data with full stats (called less frequently). */
  private async _enrichGpuStats(): Promise<void> {
    try {
      const { RustCoreIPCClient } = await import('../../../workers/continuum-core/bindings/RustCoreIPC');
      const client = await RustCoreIPCClient.getInstanceAsync();
      const stats = await client.gpuStats();
      this._gpuTotalMb = stats.totalVramMb ?? 0;
      this._gpuUsedMb = stats.totalUsedMb ?? 0;
      this._gpuName = stats.gpuName;
    } catch { /* IPC not ready */ }
  }

  /** Enrich system resource data. */
  private async _enrichSystemStats(): Promise<void> {
    try {
      const { ResourcePressureWatcher } = require('../../resources/server/ResourcePressureWatcher');
      const watcher = ResourcePressureWatcher.instance;
      if (watcher.cpu) {
        this._cpuCores = watcher.cpu.physicalCores ?? 0;
        this._cpuUsage = watcher.cpu.globalUsage ?? this._cpuUsage;
      }
      if (watcher.memory) {
        this._memoryTotalMb = Math.round((watcher.memory.totalBytes ?? 0) / (1024 * 1024));
        this._memoryUsedMb = Math.round((watcher.memory.usedBytes ?? 0) / (1024 * 1024));
        this._memoryPressure = watcher.memory.pressure ?? this._memoryPressure;
      }
    } catch { /* ok */ }
  }

  /** Write one sample to the database. Fire-and-forget. */
  private async _writeSample(): Promise<void> {
    if (!this._handle || !this._running) return;

    // Don't write if we haven't received any data yet
    if (!this._hasReceivedData) return;

    // Refresh enriched data
    await this._enrichSystemStats();
    await this._enrichGpuStats();

    try {
      const entity = SystemMetricsEntity.create({
        timestamp: Date.now(),
        cpuUsage: this._cpuUsage,
        cpuCores: this._cpuCores,
        memoryPressure: this._memoryPressure,
        memoryTotalMb: this._memoryTotalMb,
        memoryUsedMb: this._memoryUsedMb,
        gpuPressure: this._gpuPressure,
        gpuTotalMb: this._gpuTotalMb,
        gpuUsedMb: this._gpuUsedMb,
        gpuName: this._gpuName,
      });

      if (!entity.success || !entity.entity) return;

      // Fire and forget — don't await, don't care if it fails
      ORM.store<SystemMetricsEntity>(
        SystemMetricsEntity.collection as CollectionName,
        entity.entity,
        true, // suppress events
        this._handle
      ).catch(() => {}); // silently ignore write failures
    } catch {
      // Never crash the collector
    }
  }

  /** Delete samples older than RETENTION_DAYS. */
  private async _pruneOldData(): Promise<void> {
    if (!this._handle) return;

    const cutoff = Date.now() - (RETENTION_DAYS * 24 * 60 * 60 * 1000);

    try {
      // Query old records and delete them
      const result = await ORM.query<SystemMetricsEntity>({
        collection: SystemMetricsEntity.collection as CollectionName,
        filter: { timestamp: { $lt: cutoff } },
        limit: 1000, // batch deletes
      }, this._handle);

      if (result.success && result.data && result.data.length > 0) {
        for (const record of result.data) {
          ORM.remove(
            SystemMetricsEntity.collection as CollectionName,
            record.id,
            true, // suppress events
            this._handle
          ).catch(() => {});
        }
        console.log(`[MetricsCollector] Pruned ${result.data.length} samples older than ${RETENTION_DAYS} days`);
      }
    } catch {
      // Prune failure is not critical
    }
  }
}
