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

export class AICostServerCommand extends AICostCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/cost', context, subpath, commander);
  }

  async execute(params: AICostParams): Promise<AICostResult> {
    try {
      // Parse time range
      const { startTime, endTime } = this.parseTimeRange(params.startTime || '24h', params.endTime);

      console.log(`💰 ai/cost: Querying generations from ${new Date(startTime).toISOString()} to ${new Date(endTime).toISOString()}`);

      // TODO: Query AIGenerationEntity from database
      // For now, return mock data to test command structure

      const summary = {
        totalCost: 2.45,
        totalGenerations: 39,
        totalTokens: 45000,
        inputTokens: 25000,
        outputTokens: 20000,
        avgCostPerGeneration: 0.0628,
        avgTokensPerGeneration: 1154,
        avgResponseTime: 1234,
        timeRange: {
          start: new Date(startTime).toISOString(),
          end: new Date(endTime).toISOString(),
          duration: this.formatDuration(endTime - startTime)
        }
      };

      // Build cost breakdowns
      const costByProvider = params.includeBreakdown ? {
        'openai': {
          cost: 1.85,
          generations: 25,
          tokens: 30000,
          avgCostPerGeneration: 0.074,
          percentage: 75.5
        },
        'anthropic': {
          cost: 0.45,
          generations: 10,
          tokens: 12000,
          avgCostPerGeneration: 0.045,
          percentage: 18.4
        },
        'ollama': {
          cost: 0.00,
          generations: 4,
          tokens: 3000,
          avgCostPerGeneration: 0.00,
          percentage: 0.0
        }
      } : undefined;

      const costByModel = params.includeBreakdown ? {
        'gpt-4-turbo': {
          provider: 'openai',
          cost: 1.20,
          generations: 15,
          tokens: 18000,
          avgCostPerGeneration: 0.08,
          percentage: 49.0
        },
        'gpt-4o': {
          provider: 'openai',
          cost: 0.65,
          generations: 10,
          tokens: 12000,
          avgCostPerGeneration: 0.065,
          percentage: 26.5
        },
        'claude-3-5-sonnet-20241022': {
          provider: 'anthropic',
          cost: 0.45,
          generations: 10,
          tokens: 12000,
          avgCostPerGeneration: 0.045,
          percentage: 18.4
        },
        'llama-3.2-vision': {
          provider: 'ollama',
          cost: 0.00,
          generations: 4,
          tokens: 3000,
          avgCostPerGeneration: 0.00,
          percentage: 0.0
        }
      } : undefined;

      const topModels = params.includeTopModels ? [
        {
          provider: 'openai',
          model: 'gpt-4-turbo',
          cost: 1.20,
          generations: 15,
          avgCostPerGeneration: 0.08,
          percentage: 49.0
        },
        {
          provider: 'openai',
          model: 'gpt-4o',
          cost: 0.65,
          generations: 10,
          avgCostPerGeneration: 0.065,
          percentage: 26.5
        },
        {
          provider: 'anthropic',
          model: 'claude-3-5-sonnet-20241022',
          cost: 0.45,
          generations: 10,
          avgCostPerGeneration: 0.045,
          percentage: 18.4
        }
      ].slice(0, params.includeTopModels) : undefined;

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
