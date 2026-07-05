/**
 * System Metrics Command - Server Implementation
 *
 * Queries the dedicated metrics database for historical CPU/GPU/memory data.
 * Returns downsampled time-series suitable for sparkline/graph rendering.
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { SystemMetricsParams, SystemMetricsResult, SystemMetricsPoint } from '../shared/SystemMetricsTypes';
import { SystemMetricsEntity } from '@system/data/entities/SystemMetricsEntity';
import { MetricsCollector } from '@system/metrics/server/MetricsCollector';
import { ORM } from '../../../../daemons/data-daemon/server/ORM';
import type { CollectionName } from '../../../../shared/generated-collection-constants';

/** Parse relative time range to ms duration */
function parseRange(range: string): number {
  const units: Record<string, number> = {
    h: 3600_000,
    d: 86400_000,
  };
  const match = range.match(/^(\d+)([hd])$/);
  if (!match) return 3600_000; // default 1h
  return parseInt(match[1]) * (units[match[2]] ?? 3600_000);
}

export class SystemMetricsServerCommand extends CommandBase<SystemMetricsParams, SystemMetricsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('system-metrics', context, subpath, commander);
  }

  async execute(params: SystemMetricsParams): Promise<SystemMetricsResult> {
    try {
      const collector = MetricsCollector.instance;
      const handle = collector.handle;

      if (!handle) {
        return transformPayload(params, {
          success: false,
          timeSeries: [],
          current: { cpuUsage: 0, memoryPressure: 0, memoryUsedMb: 0, memoryTotalMb: 0, gpuPressure: 0, gpuUsedMb: 0, gpuTotalMb: 0 },
          totalSamples: 0,
          error: 'MetricsCollector not started',
        });
      }

      const range = params.range ?? '1h';
      const maxPoints = params.maxPoints ?? 120;
      const durationMs = parseRange(range);
      const cutoff = Date.now() - durationMs;

      // Query all samples in range, ordered by timestamp
      const result = await ORM.query<SystemMetricsEntity>({
        collection: SystemMetricsEntity.collection as CollectionName,
        filter: { timestamp: { $gte: cutoff } },
        sort: [{ field: 'timestamp', direction: 'asc' }],
        limit: 10000, // cap to prevent memory issues
      }, handle);

      let samples: SystemMetricsPoint[] = [];
      if (result.success && result.data) {
        samples = result.data.map(r => {
          const d = (r as any).data ?? r;
          return {
            timestamp: d.timestamp ?? 0,
            cpuUsage: d.cpuUsage ?? 0,
            memoryPressure: d.memoryPressure ?? 0,
            memoryUsedMb: d.memoryUsedMb ?? 0,
            memoryTotalMb: d.memoryTotalMb ?? 0,
            gpuPressure: d.gpuPressure ?? 0,
            gpuUsedMb: d.gpuUsedMb ?? 0,
            gpuTotalMb: d.gpuTotalMb ?? 0,
          };
        });
      }

      // Downsample if too many points
      const timeSeries = samples.length > maxPoints
        ? this.downsample(samples, maxPoints)
        : samples;

      // Get total count for context
      const countResult = await ORM.count({
        collection: SystemMetricsEntity.collection as CollectionName,
      }, handle);

      const totalSamples = countResult.success ? (countResult.data ?? 0) : 0;

      // Current values = latest sample or zeros
      const latest = samples.length > 0 ? samples[samples.length - 1] : null;

      return transformPayload(params, {
        success: true,
        timeSeries,
        current: {
          cpuUsage: latest?.cpuUsage ?? 0,
          memoryPressure: latest?.memoryPressure ?? 0,
          memoryUsedMb: latest?.memoryUsedMb ?? 0,
          memoryTotalMb: latest?.memoryTotalMb ?? 0,
          gpuPressure: latest?.gpuPressure ?? 0,
          gpuUsedMb: latest?.gpuUsedMb ?? 0,
          gpuTotalMb: latest?.gpuTotalMb ?? 0,
        },
        totalSamples,
      });
    } catch (error) {
      return transformPayload(params, {
        success: false,
        timeSeries: [],
        current: { cpuUsage: 0, memoryPressure: 0, memoryUsedMb: 0, memoryTotalMb: 0, gpuPressure: 0, gpuUsedMb: 0, gpuTotalMb: 0 },
        totalSamples: 0,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }

  /** Downsample by averaging points in each bucket */
  private downsample(samples: SystemMetricsPoint[], maxPoints: number): SystemMetricsPoint[] {
    const bucketSize = Math.ceil(samples.length / maxPoints);
    const result: SystemMetricsPoint[] = [];

    for (let i = 0; i < samples.length; i += bucketSize) {
      const bucket = samples.slice(i, i + bucketSize);
      const n = bucket.length;
      result.push({
        timestamp: bucket[Math.floor(n / 2)].timestamp, // midpoint timestamp
        cpuUsage: bucket.reduce((s, p) => s + p.cpuUsage, 0) / n,
        memoryPressure: bucket.reduce((s, p) => s + p.memoryPressure, 0) / n,
        memoryUsedMb: bucket.reduce((s, p) => s + p.memoryUsedMb, 0) / n,
        memoryTotalMb: bucket[n - 1].memoryTotalMb, // latest total
        gpuPressure: bucket.reduce((s, p) => s + p.gpuPressure, 0) / n,
        gpuUsedMb: bucket.reduce((s, p) => s + p.gpuUsedMb, 0) / n,
        gpuTotalMb: bucket[n - 1].gpuTotalMb,
      });
    }

    return result;
  }
}
