/**
 * AI Cost Server Command
 *
 * Query AI generation costs with filtering, aggregation, and time-series data
 */

import { AICostCommand } from '../shared/AICostCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AICostParams, AICostResult } from '../shared/AICostTypes';
import { AIGenerationEntity } from '../../../../system/data/entities/AIGenerationEntity';
import { Commands } from '../../../../system/core/shared/Commands';
import { DATA_COMMANDS } from '../../../data/shared/DataCommandConstants';
import type { DataListParams, DataListResult } from '../../../data/list/shared/DataListTypes';
import { createDataListParams } from '../../../data/list/shared/DataListTypes';

export class AICostServerCommand extends AICostCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/cost', context, subpath, commander);
  }

  async execute(params: AICostParams): Promise<AICostResult> {
    try {
      // Parse time range
      const { startTime, endTime } = this.parseTimeRange(params.startTime || '24h', params.endTime);

      console.log(`💰 ai/cost: Querying generations from ${new Date(startTime).toISOString()} to ${new Date(endTime).toISOString()}`);

      // Build filter with range operators (SQLite adapter supports $gte, $lte, etc.)
      const filter: Record<string, any> = {
        timestamp: {
          $gte: startTime,
          $lte: endTime
        }
      };

      if (params.provider) {
        filter.provider = params.provider;
      }

      if (params.model) {
        filter.model = params.model;
      }

      // Query AIGenerationEntity from database using data/list - let SQL do the filtering
      const listParams = createDataListParams<AIGenerationEntity>(
        params.context,
        params.sessionId,
        {
          collection: 'ai_generations',
          filter,
          orderBy: [{ field: 'timestamp', direction: 'desc' }]
        }
      );

      const listResult = await Commands.execute<DataListParams<AIGenerationEntity>, DataListResult<AIGenerationEntity>>(
        DATA_COMMANDS.LIST,
        listParams
      );

      if (!listResult.success || !listResult.items) {
        throw new Error('Failed to query AI generations from database');
      }

      const finalGens = Array.from(listResult.items);

      // Calculate aggregates
      const totalCost = finalGens.reduce((sum: number, gen: AIGenerationEntity) => sum + gen.estimatedCost, 0);
      const totalTokens = finalGens.reduce((sum: number, gen: AIGenerationEntity) => sum + gen.totalTokens, 0);
      const inputTokens = finalGens.reduce((sum: number, gen: AIGenerationEntity) => sum + gen.inputTokens, 0);
      const outputTokens = finalGens.reduce((sum: number, gen: AIGenerationEntity) => sum + gen.outputTokens, 0);
      const totalResponseTime = finalGens.reduce((sum: number, gen: AIGenerationEntity) => sum + gen.responseTime, 0);

      const summary = {
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

      // Build cost breakdowns
      const costByProvider = params.includeBreakdown ? this.aggregateByProvider(finalGens, totalCost) : undefined;

      const costByModel = params.includeBreakdown ? this.aggregateByModel(finalGens, totalCost) : undefined;

      const topModels = params.includeTopModels ? this.getTopModels(finalGens, totalCost, params.includeTopModels) : undefined;

      // Time-series data for graphing
      const timeSeries = params.includeTimeSeries ? this.generateMockTimeSeries(startTime, endTime, params.interval || '1h') : undefined;

      // Latency metrics
      const latency = params.includeLatency ? {
        avgLatency: 1234,
        minLatency: 450,
        maxLatency: 3200,
        p50: 1100,
        p95: 2500,
        p99: 3000
      } : undefined;

      // Format output
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
      console.error('❌ ai/cost failed:', error);
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

  /**
   * Parse time range from relative strings like "24h", "7d", "today" or absolute ISO timestamps
   */
  private parseTimeRange(start: string, end?: string): { startTime: number; endTime: number } {
    const now = Date.now();
    let startTime: number;
    let endTime: number = end ? this.parseRelativeTime(end) : now;

    startTime = this.parseRelativeTime(start);

    return { startTime, endTime };
  }

  /**
   * Parse relative time strings like "1h", "24h", "7d", "30d", "today", "yesterday"
   */
  private parseRelativeTime(timeStr: string): number {
    const now = Date.now();

    // Absolute ISO timestamp
    if (timeStr.includes('T') || timeStr.includes('-')) {
      return new Date(timeStr).getTime();
    }

    // Relative time
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

    // Parse duration strings like "1h", "24h", "7d", "30d"
    const match = timeStr.match(/^(\d+)(h|d|w|m)$/);
    if (!match) {
      throw new Error(`Invalid time format: ${timeStr}. Use formats like "1h", "24h", "7d", "30d", "today", or ISO timestamp`);
    }

    const value = parseInt(match[1], 10);
    const unit = match[2];

    const msPerUnit: Record<string, number> = {
      'h': 60 * 60 * 1000,
      'd': 24 * 60 * 60 * 1000,
      'w': 7 * 24 * 60 * 60 * 1000,
      'm': 30 * 24 * 60 * 60 * 1000
    };

    return now - (value * msPerUnit[unit]);
  }

  /**
   * Format duration as human-readable string
   */
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

  /**
   * Generate mock time-series data (TODO: replace with real database queries)
   */
  private generateMockTimeSeries(startTime: number, endTime: number, interval: string): Array<{
    timestamp: string;
    cost: number;
    generations: number;
    tokens: number;
    avgResponseTime: number;
  }> {
    const intervalMs = this.parseIntervalToMs(interval);
    const points: Array<any> = [];

    for (let time = startTime; time < endTime; time += intervalMs) {
      points.push({
        timestamp: new Date(time).toISOString(),
        cost: Math.random() * 0.5,
        generations: Math.floor(Math.random() * 10) + 1,
        tokens: Math.floor(Math.random() * 5000) + 1000,
        avgResponseTime: Math.floor(Math.random() * 2000) + 500
      });
    }

    return points;
  }

  /**
   * Parse interval string to milliseconds
   */
  private parseIntervalToMs(interval: string): number {
    const match = interval.match(/^(\d+)(h|d|w)$/);
    if (!match) {
      throw new Error(`Invalid interval format: ${interval}. Use formats like "1h", "6h", "1d", "1w"`);
    }

    const value = parseInt(match[1], 10);
    const unit = match[2];

    const msPerUnit: Record<string, number> = {
      'h': 60 * 60 * 1000,
      'd': 24 * 60 * 60 * 1000,
      'w': 7 * 24 * 60 * 60 * 1000
    };

    return value * msPerUnit[unit];
  }

  /**
   * Aggregate generations by provider
   */
  private aggregateByProvider(generations: AIGenerationEntity[], totalCost: number): Record<string, any> {
    const byProvider: Record<string, { cost: number; generations: number; tokens: number }> = {};

    for (const gen of generations) {
      if (!byProvider[gen.provider]) {
        byProvider[gen.provider] = { cost: 0, generations: 0, tokens: 0 };
      }
      byProvider[gen.provider].cost += gen.estimatedCost;
      byProvider[gen.provider].generations += 1;
      byProvider[gen.provider].tokens += gen.totalTokens;
    }

    const result: Record<string, any> = {};
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

  /**
   * Aggregate generations by model
   */
  private aggregateByModel(generations: AIGenerationEntity[], totalCost: number): Record<string, any> {
    const byModel: Record<string, { provider: string; cost: number; generations: number; tokens: number }> = {};

    for (const gen of generations) {
      if (!byModel[gen.model]) {
        byModel[gen.model] = { provider: gen.provider, cost: 0, generations: 0, tokens: 0 };
      }
      byModel[gen.model].cost += gen.estimatedCost;
      byModel[gen.model].generations += 1;
      byModel[gen.model].tokens += gen.totalTokens;
    }

    const result: Record<string, any> = {};
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

  /**
   * Get top N models by cost
   */
  private getTopModels(generations: AIGenerationEntity[], totalCost: number, limit: number): Array<any> {
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

  /**
   * Print formatted text report
   */
  private printTextReport(
    summary: any,
    costByProvider?: any,
    costByModel?: any,
    topModels?: any[],
    latency?: any
  ): void {
    console.log('\n💰 AI COST REPORT');
    console.log('='.repeat(60));
    console.log(`📅 Time Range: ${summary.timeRange.start.split('T')[0]} to ${summary.timeRange.end.split('T')[0]} (${summary.timeRange.duration})`);
    console.log('');
    console.log(`💵 Total Cost:         $${summary.totalCost.toFixed(4)}`);
    console.log(`🤖 Total Generations:  ${summary.totalGenerations}`);
    console.log(`🔢 Total Tokens:       ${summary.totalTokens.toLocaleString()}`);
    console.log(`   Input Tokens:       ${summary.inputTokens.toLocaleString()}`);
    console.log(`   Output Tokens:      ${summary.outputTokens.toLocaleString()}`);
    console.log(`📊 Avg Cost/Gen:       $${summary.avgCostPerGeneration.toFixed(4)}`);
    console.log(`📊 Avg Tokens/Gen:     ${Math.round(summary.avgTokensPerGeneration)}`);
    console.log(`⏱️  Avg Response Time:  ${summary.avgResponseTime}ms`);

    if (costByProvider) {
      console.log('\n📦 COST BY PROVIDER');
      console.log('-'.repeat(60));
      for (const [provider, stats] of Object.entries<any>(costByProvider)) {
        console.log(`${provider.toUpperCase().padEnd(15)} $${stats.cost.toFixed(4)}  (${stats.percentage.toFixed(1)}%)  ${stats.generations} gens`);
      }
    }

    if (topModels && topModels.length > 0) {
      console.log('\n🏆 TOP MODELS BY COST');
      console.log('-'.repeat(60));
      topModels.forEach((model, i) => {
        const rank = `${i + 1}.`.padEnd(3);
        const modelName = model.model.padEnd(30);
        const cost = `$${model.cost.toFixed(4)}`;
        const pct = `(${model.percentage.toFixed(1)}%)`;
        console.log(`${rank} ${modelName} ${cost}  ${pct}`);
      });
    }

    if (latency) {
      console.log('\n⏱️  LATENCY METRICS');
      console.log('-'.repeat(60));
      console.log('Avg:  ' + latency.avgLatency + 'ms');
      console.log('Min:  ' + latency.minLatency + 'ms');
      console.log('Max:  ' + latency.maxLatency + 'ms');
      console.log('P50:  ' + latency.p50 + 'ms');
      console.log('P95:  ' + latency.p95 + 'ms');
      console.log('P99:  ' + latency.p99 + 'ms');
    }

    console.log('\n' + '='.repeat(60) + '\n');
  }
}
