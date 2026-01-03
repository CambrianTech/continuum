/**
 * TimingServerCommand - Analyze Rust worker timing metrics
 *
 * Reads timing data from /tmp/jtag-data-daemon-timing.jsonl
 * Provides percentile analysis for performance monitoring.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type {
  TimingParams,
  TimingResult,
  RustTimingRecord,
  PercentileStats,
  RequestTypeStats
} from '../shared/TimingTypes';
import * as fs from 'fs';
import * as readline from 'readline';

const TIMING_FILE = '/tmp/jtag-data-daemon-timing.jsonl';

export class TimingServerCommand extends CommandBase<TimingParams, TimingResult> {
  static readonly commandName = 'development/timing';
  readonly name = 'development/timing';
  readonly description = 'Analyze Rust worker timing metrics with percentile breakdowns';

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/timing', context, subpath, commander);
  }

  async execute(params: TimingParams): Promise<TimingResult> {
    const windowMinutes = params.windowMinutes ?? 60;
    const cutoffMs = windowMinutes > 0 ? Date.now() - (windowMinutes * 60 * 1000) : 0;

    try {
      // Check if timing file exists
      if (!fs.existsSync(TIMING_FILE)) {
        return this.errorResult(params, 'Timing file not found. Rust worker may not be running.');
      }

      // Read and filter records
      const records = await this.readTimingRecords(cutoffMs, params.requestType);

      if (records.length === 0) {
        return this.errorResult(params, 'No timing records found in the specified window.');
      }

      // Group by request type
      const byType = new Map<string, RustTimingRecord[]>();
      for (const record of records) {
        const existing = byType.get(record.request_type) || [];
        existing.push(record);
        byType.set(record.request_type, existing);
      }

      // Calculate stats for each type
      const typeStats: RequestTypeStats[] = [];
      for (const [type, typeRecords] of byType) {
        const stats = this.calculateStats(typeRecords, params.showBreakdown ?? false);
        typeStats.push({ type, ...stats });
      }

      // Sort by count descending
      typeStats.sort((a, b) => b.count - a.count);

      // Calculate overall stats
      const allConcurrent = records.map(r => r.concurrent_requests);
      const successCount = records.filter(r => r.success).length;

      // Generate recommendations
      const recommendations = this.generateRecommendations(typeStats);

      // Clear if requested
      if (params.clear) {
        fs.truncateSync(TIMING_FILE, 0);
      }

      return {
        context: params.context,
        sessionId: params.sessionId,
        success: true,
        windowMinutes,
        recordCount: records.length,
        oldestRecord: Math.min(...records.map(r => r.timestamp_ms)),
        newestRecord: Math.max(...records.map(r => r.timestamp_ms)),
        byType: typeStats,
        overall: {
          totalRequests: records.length,
          successRate: (successCount / records.length) * 100,
          avgConcurrentRequests: this.avg(allConcurrent),
          maxConcurrentRequests: Math.max(...allConcurrent)
        },
        recommendations
      };
    } catch (error) {
      return this.errorResult(params, `Failed to analyze timing: ${error}`);
    }
  }

  private async readTimingRecords(
    cutoffMs: number,
    requestType?: string
  ): Promise<RustTimingRecord[]> {
    const records: RustTimingRecord[] = [];

    const fileStream = fs.createReadStream(TIMING_FILE);
    const rl = readline.createInterface({
      input: fileStream,
      crlfDelay: Infinity
    });

    for await (const line of rl) {
      if (!line.trim()) continue;
      try {
        const record: RustTimingRecord = JSON.parse(line);
        if (record.timestamp_ms >= cutoffMs) {
          if (!requestType || record.request_type === requestType) {
            records.push(record);
          }
        }
      } catch {
        // Skip malformed lines
      }
    }

    return records;
  }

  private calculateStats(
    records: RustTimingRecord[],
    includeBreakdown: boolean
  ): Omit<RequestTypeStats, 'type'> {
    const toMs = (ns: number) => ns / 1_000_000;

    // Sort records by total_ns for percentile calculation
    const sorted = [...records].sort((a, b) => a.total_ns - b.total_ns);

    const total_ms = this.percentileStats(sorted.map(r => toMs(r.total_ns)));
    const execute_ms = this.percentileStats(sorted.map(r => toMs(r.execute_ns)));

    const result: Omit<RequestTypeStats, 'type'> = {
      count: records.length,
      total_ms,
      execute_ms
    };

    if (includeBreakdown) {
      result.breakdown = {
        socket_read_ms: this.percentileStats(records.map(r => toMs(r.socket_read_ns))),
        parse_ms: this.percentileStats(records.map(r => toMs(r.parse_ns))),
        route_ms: this.percentileStats(records.map(r => toMs(r.route_ns))),
        lock_wait_ms: this.percentileStats(records.map(r => toMs(r.lock_wait_ns))),
        execute_ms,
        serialize_ms: this.percentileStats(records.map(r => toMs(r.serialize_ns))),
        socket_write_ms: this.percentileStats(records.map(r => toMs(r.socket_write_ns))),
        total_ms
      };
    }

    return result;
  }

  private percentileStats(values: number[]): PercentileStats {
    if (values.length === 0) {
      return { count: 0, avg: 0, min: 0, max: 0, p50: 0, p95: 0, p99: 0 };
    }

    const sorted = [...values].sort((a, b) => a - b);
    const n = sorted.length;

    return {
      count: n,
      avg: Math.round(this.avg(sorted) * 100) / 100,
      min: Math.round(sorted[0] * 100) / 100,
      max: Math.round(sorted[n - 1] * 100) / 100,
      p50: Math.round(sorted[Math.floor(n * 0.5)] * 100) / 100,
      p95: Math.round(sorted[Math.floor(n * 0.95)] * 100) / 100,
      p99: Math.round(sorted[Math.floor(n * 0.99)] * 100) / 100
    };
  }

  private avg(values: number[]): number {
    if (values.length === 0) return 0;
    return values.reduce((a, b) => a + b, 0) / values.length;
  }

  private generateRecommendations(stats: RequestTypeStats[]): string[] {
    const recommendations: string[] = [];

    for (const stat of stats) {
      // High p99 latency
      if (stat.total_ms.p99 > 1000) {
        recommendations.push(
          `${stat.type}: P99 latency is ${stat.total_ms.p99}ms - investigate slow requests`
        );
      }

      // High variance (p99 >> p50)
      if (stat.total_ms.p99 > stat.total_ms.p50 * 10 && stat.total_ms.p50 > 1) {
        recommendations.push(
          `${stat.type}: High variance (P50=${stat.total_ms.p50}ms, P99=${stat.total_ms.p99}ms)`
        );
      }

      // Low throughput
      if (stat.count < 10 && stat.type !== 'ping') {
        recommendations.push(
          `${stat.type}: Only ${stat.count} samples - collect more data for reliable stats`
        );
      }
    }

    if (recommendations.length === 0) {
      recommendations.push('All metrics look healthy');
    }

    return recommendations;
  }

  private errorResult(params: TimingParams, error: string): TimingResult {
    return {
      context: params.context,
      sessionId: params.sessionId,
      success: false,
      error,
      windowMinutes: 0,
      recordCount: 0,
      byType: [],
      overall: {
        totalRequests: 0,
        successRate: 0,
        avgConcurrentRequests: 0,
        maxConcurrentRequests: 0
      },
      recommendations: []
    };
  }
}
