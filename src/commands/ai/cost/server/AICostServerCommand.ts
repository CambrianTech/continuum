/**
 * AI Cost Server Command
 *
 * Query AI generation costs with filtering, aggregation, and time-series data.
 * Reads from the metrics SQLite database (same store as system_metrics).
 */

import { AICostCommand } from '../shared/AICostCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AICostParams, AICostResult } from '../shared/AICostTypes';
import { AIGenerationEntity } from '../../../../system/data/entities/AIGenerationEntity';
import { ORM } from '../../../../daemons/data-daemon/server/ORM';
import type { DataRecord, UniversalFilter } from '../../../../daemons/data-daemon/shared/DataStorageAdapter';
import type { CollectionName } from '../../../../shared/generated-collection-constants';
import { MetricsCollector } from '../../../../system/metrics/server/MetricsCollector';

// ── Breakdown types (match AICostResult shape) ────────────────────────────

interface ProviderStats {
  cost: number;
  generations: number;
  tokens: number;
  avgCostPerGeneration: number;
  percentage: number;
}

interface ModelStats extends ProviderStats {
  provider: string;
}

interface TopModel {
  provider: string;
  model: string;
  cost: number;
  generations: number;
  avgCostPerGeneration: number;
  percentage: number;
}

interface LatencyMetrics {
  avgLatency: number;
  minLatency: number;
  maxLatency: number;
  p50: number;
  p95: number;
  p99: number;
}

interface TimeSeriesPoint {
  timestamp: string;
  cost: number;
  generations: number;
  tokens: number;
  avgResponseTime: number;
}

// ── Command ───────────────────────────────────────────────────────────────

export class AICostServerCommand extends AICostCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/cost', context, subpath, commander);
  }

  async execute(params: AICostParams): Promise<AICostResult> {
    try {
      const { startTime, endTime } = this.parseTimeRange(params.startTime || '24h', params.endTime);

      // Rust ORM FieldFilter is a single-operator enum per field — combining
      // $gte + $lte on the same field silently fails (serde falls through to Value).
      // Use $gte only, then filter endTime in TS.
      const filter: UniversalFilter = {
        timestamp: { $gte: startTime },
        ...(params.provider ? { provider: params.provider } : {}),
        ...(params.model ? { model: params.model } : {}),
      };

      const handle = MetricsCollector.instance.handle;
      if (!handle) {
        throw new Error('MetricsCollector not started — no metrics database handle');
      }

      const queryResult = await ORM.query<AIGenerationEntity>({
        collection: AIGenerationEntity.collection as CollectionName,
        filter,
        sort: [{ field: 'timestamp', direction: 'desc' }],
        limit: 10000,
      }, handle);

      if (!queryResult.success || !queryResult.data) {
        throw new Error('Failed to query AI generations from metrics database');
      }

      // ORM.query returns DataRecord[] — Rust IPC may nest entity under .data or flatten it.
      // Same unwrap pattern as SystemMetricsServerCommand.
      const finalGens: AIGenerationEntity[] = queryResult.data
        .map((r) => {
          const record = r as DataRecord<AIGenerationEntity>;
          return record.data ?? (r as unknown as AIGenerationEntity);
        })
        .filter((gen) => gen.timestamp <= endTime);

      // Calculate aggregates
      const totalCost = finalGens.reduce((sum, gen) => sum + gen.estimatedCost, 0);
      const totalTokens = finalGens.reduce((sum, gen) => sum + gen.totalTokens, 0);
      const inputTokens = finalGens.reduce((sum, gen) => sum + gen.inputTokens, 0);
      const outputTokens = finalGens.reduce((sum, gen) => sum + gen.outputTokens, 0);
      const totalResponseTime = finalGens.reduce((sum, gen) => sum + gen.responseTimeMs, 0);

      const summary: AICostResult['summary'] = {
        totalCost,
        totalGenerations: finalGens.length,
        totalTokens,
        inputTokens,
        outputTokens,
        avgCostPerGeneration: finalGens.length > 0 ? totalCost / finalGens.length : 0,
        avgTokensPerGeneration: finalGens.length > 0 ? totalTokens / finalGens.length : 0,
        avgResponseTime: finalGens.length > 0 ? Math.round(totalResponseTime / finalGens.length) : 0,
        timeRange: {
          start: new Date(startTime).toISOString(),
          end: new Date(endTime).toISOString(),
          duration: this.formatDuration(endTime - startTime)
        }
      };

      const costByProvider = params.includeBreakdown ? this.aggregateByProvider(finalGens, totalCost) : undefined;
      const costByModel = params.includeBreakdown ? this.aggregateByModel(finalGens, totalCost) : undefined;
      const topModels = params.includeTopModels ? this.getTopModels(finalGens, totalCost, params.includeTopModels) : undefined;
      const timeSeries = params.includeTimeSeries ? this.generateTimeSeries(finalGens, startTime, endTime, params.interval || '1h') : undefined;
      const latency = params.includeLatency ? this.calculateLatencyMetrics(finalGens) : undefined;

      if (params.format === 'text') {
        this.printTextReport(summary, costByProvider, costByModel, topModels, latency);
      }

      return {
        context: params.context,
        sessionId: params.sessionId,
        success: true,
        summary,
        costByProvider,
        costByModel,
        topModels,
        timeSeries,
        latency
      };
    } catch (error) {
      console.error('ai/cost failed:', error);
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        error: error instanceof Error ? error.message : String(error),
        summary: {
          totalCost: 0,
          totalGenerations: 0,
          totalTokens: 0,
          inputTokens: 0,
          outputTokens: 0,
          avgCostPerGeneration: 0,
          avgTokensPerGeneration: 0,
          avgResponseTime: 0,
          timeRange: {
            start: new Date().toISOString(),
            end: new Date().toISOString(),
            duration: '0s'
          }
        }
      };
    }
  }

  // ── Time parsing ──────────────────────────────────────────────────────────

  private parseTimeRange(start: string, end?: string): { startTime: number; endTime: number } {
    const endTime = end ? this.parseRelativeTime(end) : Date.now();
    const startTime = this.parseRelativeTime(start);
    return { startTime, endTime };
  }

  private parseRelativeTime(timeStr: string): number {
    const now = Date.now();

    if (timeStr.includes('T') || timeStr.includes('-')) {
      return new Date(timeStr).getTime();
    }

    if (timeStr === 'now') return now;
    if (timeStr === 'today') {
      const today = new Date();
      today.setHours(0, 0, 0, 0);
      return today.getTime();
    }
    if (timeStr === 'yesterday') {
      const yesterday = new Date();
      yesterday.setDate(yesterday.getDate() - 1);
      yesterday.setHours(0, 0, 0, 0);
      return yesterday.getTime();
    }

    const match = timeStr.match(/^(\d+)(h|d|w|m)$/);
    if (!match) {
      throw new Error(`Invalid time format: ${timeStr}. Use "1h", "24h", "7d", "30d", "today", or ISO timestamp`);
    }

    const value = parseInt(match[1], 10);
    const unit = match[2];
    const msPerUnit: Record<string, number> = {
      'h': 3_600_000,
      'd': 86_400_000,
      'w': 604_800_000,
      'm': 2_592_000_000
    };

    return now - (value * msPerUnit[unit]);
  }

  private formatDuration(ms: number): string {
    const seconds = Math.floor(ms / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);

    if (days > 0) return `${days}d ${hours % 24}h`;
    if (hours > 0) return `${hours}h ${minutes % 60}m`;
    if (minutes > 0) return `${minutes}m ${seconds % 60}s`;
    return `${seconds}s`;
  }

  // ── Latency metrics ───────────────────────────────────────────────────────

  private calculateLatencyMetrics(generations: AIGenerationEntity[]): LatencyMetrics {
    if (generations.length === 0) {
      return { avgLatency: 0, minLatency: 0, maxLatency: 0, p50: 0, p95: 0, p99: 0 };
    }

    const responseTimes = generations.map(g => g.responseTimeMs).sort((a, b) => a - b);
    const sum = responseTimes.reduce((acc, val) => acc + val, 0);

    const getPercentile = (p: number): number => {
      const index = Math.ceil((p / 100) * responseTimes.length) - 1;
      return responseTimes[Math.max(0, index)];
    };

    return {
      avgLatency: Math.round(sum / responseTimes.length),
      minLatency: responseTimes[0],
      maxLatency: responseTimes[responseTimes.length - 1],
      p50: getPercentile(50),
      p95: getPercentile(95),
      p99: getPercentile(99)
    };
  }

  // ── Time-series generation ────────────────────────────────────────────────

  private generateTimeSeries(
    generations: AIGenerationEntity[],
    startTime: number,
    endTime: number,
    interval: string
  ): TimeSeriesPoint[] {
    const intervalMs = this.parseIntervalToMs(interval);
    const points: TimeSeriesPoint[] = [];

    for (let bucketStart = startTime; bucketStart < endTime; bucketStart += intervalMs) {
      const bucketEnd = bucketStart + intervalMs;
      const bucketGens = generations.filter(g => g.timestamp >= bucketStart && g.timestamp < bucketEnd);

      const cost = bucketGens.reduce((sum, g) => sum + g.estimatedCost, 0);
      const tokens = bucketGens.reduce((sum, g) => sum + g.totalTokens, 0);
      const totalResponseTime = bucketGens.reduce((sum, g) => sum + g.responseTimeMs, 0);

      points.push({
        timestamp: new Date(bucketStart).toISOString(),
        cost,
        generations: bucketGens.length,
        tokens,
        avgResponseTime: bucketGens.length > 0 ? Math.round(totalResponseTime / bucketGens.length) : 0
      });
    }

    return points;
  }

  private parseIntervalToMs(interval: string): number {
    const match = interval.match(/^(\d+)(m|h|d|w)$/);
    if (!match) {
      throw new Error(`Invalid interval format: ${interval}. Use "5m", "1h", "6h", "1d", "1w"`);
    }

    const value = parseInt(match[1], 10);
    const unit = match[2];
    const msPerUnit: Record<string, number> = {
      'm': 60_000,
      'h': 3_600_000,
      'd': 86_400_000,
      'w': 604_800_000
    };

    return value * msPerUnit[unit];
  }

  // ── Aggregation ───────────────────────────────────────────────────────────

  private aggregateByProvider(
    generations: AIGenerationEntity[],
    totalCost: number
  ): Record<string, ProviderStats> {
    const byProvider: Record<string, { cost: number; generations: number; tokens: number }> = {};

    for (const gen of generations) {
      const entry = byProvider[gen.provider] ??= { cost: 0, generations: 0, tokens: 0 };
      entry.cost += gen.estimatedCost;
      entry.generations += 1;
      entry.tokens += gen.totalTokens;
    }

    const result: Record<string, ProviderStats> = {};
    for (const [provider, stats] of Object.entries(byProvider)) {
      result[provider] = {
        cost: stats.cost,
        generations: stats.generations,
        tokens: stats.tokens,
        avgCostPerGeneration: stats.generations > 0 ? stats.cost / stats.generations : 0,
        percentage: totalCost > 0 ? (stats.cost / totalCost) * 100 : 0
      };
    }

    return result;
  }

  private aggregateByModel(
    generations: AIGenerationEntity[],
    totalCost: number
  ): Record<string, ModelStats> {
    const byModel: Record<string, { provider: string; cost: number; generations: number; tokens: number }> = {};

    for (const gen of generations) {
      const entry = byModel[gen.model] ??= { provider: gen.provider, cost: 0, generations: 0, tokens: 0 };
      entry.cost += gen.estimatedCost;
      entry.generations += 1;
      entry.tokens += gen.totalTokens;
    }

    const result: Record<string, ModelStats> = {};
    for (const [model, stats] of Object.entries(byModel)) {
      result[model] = {
        provider: stats.provider,
        cost: stats.cost,
        generations: stats.generations,
        tokens: stats.tokens,
        avgCostPerGeneration: stats.generations > 0 ? stats.cost / stats.generations : 0,
        percentage: totalCost > 0 ? (stats.cost / totalCost) * 100 : 0
      };
    }

    return result;
  }

  private getTopModels(
    generations: AIGenerationEntity[],
    totalCost: number,
    limit: number
  ): TopModel[] {
    const byModel = this.aggregateByModel(generations, totalCost);

    return Object.entries(byModel)
      .map(([model, stats]) => ({
        provider: stats.provider,
        model,
        cost: stats.cost,
        generations: stats.generations,
        avgCostPerGeneration: stats.avgCostPerGeneration,
        percentage: stats.percentage
      }))
      .sort((a, b) => b.cost - a.cost)
      .slice(0, limit);
  }

  // ── Text report ───────────────────────────────────────────────────────────

  private printTextReport(
    summary: AICostResult['summary'],
    costByProvider?: Record<string, ProviderStats>,
    costByModel?: Record<string, ModelStats>,
    topModels?: TopModel[],
    latency?: LatencyMetrics
  ): void {
    const lines: string[] = [
      '\nAI COST REPORT',
      '='.repeat(60),
      `Time Range: ${summary.timeRange.start.split('T')[0]} to ${summary.timeRange.end.split('T')[0]} (${summary.timeRange.duration})`,
      '',
      `Total Cost:         $${summary.totalCost.toFixed(4)}`,
      `Total Generations:  ${summary.totalGenerations}`,
      `Total Tokens:       ${summary.totalTokens.toLocaleString()}`,
      `  Input Tokens:     ${summary.inputTokens.toLocaleString()}`,
      `  Output Tokens:    ${summary.outputTokens.toLocaleString()}`,
      `Avg Cost/Gen:       $${summary.avgCostPerGeneration.toFixed(4)}`,
      `Avg Tokens/Gen:     ${Math.round(summary.avgTokensPerGeneration)}`,
      `Avg Response Time:  ${summary.avgResponseTime}ms`,
    ];

    if (costByProvider) {
      lines.push('\nCOST BY PROVIDER', '-'.repeat(60));
      for (const [provider, stats] of Object.entries(costByProvider)) {
        lines.push(`${provider.toUpperCase().padEnd(15)} $${stats.cost.toFixed(4)}  (${stats.percentage.toFixed(1)}%)  ${stats.generations} gens`);
      }
    }

    if (topModels && topModels.length > 0) {
      lines.push('\nTOP MODELS BY COST', '-'.repeat(60));
      topModels.forEach((model, i) => {
        lines.push(`${`${i + 1}.`.padEnd(3)} ${model.model.padEnd(30)} $${model.cost.toFixed(4)}  (${model.percentage.toFixed(1)}%)`);
      });
    }

    if (latency) {
      lines.push('\nLATENCY METRICS', '-'.repeat(60));
      lines.push(`Avg:  ${latency.avgLatency}ms`, `Min:  ${latency.minLatency}ms`, `Max:  ${latency.maxLatency}ms`);
      lines.push(`P50:  ${latency.p50}ms`, `P95:  ${latency.p95}ms`, `P99:  ${latency.p99}ms`);
    }

    lines.push('\n' + '='.repeat(60) + '\n');
    // Console output for CLI text format (--format=text)
    console.log(lines.join('\n'));
  }
}
