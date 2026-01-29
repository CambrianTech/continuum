/**
 * AI Cost Command Types
 *
 * Query and visualize AI generation costs with filtering and time-series data
 */

import type { CommandParams, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

export interface AICostParams extends CommandParams {
  // Time range filtering
  startTime?: string;              // ISO timestamp or relative ("1h", "24h", "7d", "30d", "today", "yesterday")
  endTime?: string;                // ISO timestamp or relative (default: now)

  // Entity filtering
  provider?: string;               // Filter by provider: "openai", "anthropic", "ollama", "deepseek"
  model?: string;                  // Filter by model: "gpt-4", "claude-3-opus", etc.
  userId?: UUID;                   // Filter by user (human or AI)
  roomId?: UUID;                   // Filter by room
  purpose?: string;                // Filter by purpose: "chat", "should-respond", "rag", etc.

  // Aggregation options
  groupBy?: 'provider' | 'model' | 'user' | 'room' | 'purpose' | 'hour' | 'day';
  interval?: string;               // Time-series interval: "1h", "6h", "1d", "1w"

  // Analysis options
  includeTimeSeries?: boolean;     // Include time-series data for graphing
  includeBreakdown?: boolean;      // Include cost breakdown by provider/model
  includeTopModels?: number;       // Include top N most expensive models (default: 10)
  includeLatency?: boolean;        // Include latency metrics (avg, p50, p95, p99)

  // Output control
  format?: 'text' | 'json' | 'csv'; // Output format
  limit?: number;                  // Max results to return
}

export interface AICostResult {
  context: JTAGContext;
  sessionId: UUID;

  success: boolean;
  error?: string;

  // Summary statistics
  summary: {
    totalCost: number;             // Total cost in USD
    totalGenerations: number;      // Total number of generations
    totalTokens: number;           // Total tokens (input + output)
    inputTokens: number;           // Total input tokens
    outputTokens: number;          // Total output tokens
    avgCostPerGeneration: number;  // Average cost per generation
    avgTokensPerGeneration: number; // Average tokens per generation
    avgResponseTime: number;       // Average response time in ms
    timeRange: {
      start: string;               // ISO timestamp
      end: string;                 // ISO timestamp
      duration: string;            // Human-readable duration
    };
  };

  // Cost breakdown by provider
  costByProvider?: {
    [provider: string]: {
      cost: number;
      generations: number;
      tokens: number;
      avgCostPerGeneration: number;
      percentage: number;          // % of total cost
    };
  };

  // Cost breakdown by model
  costByModel?: {
    [model: string]: {
      provider: string;
      cost: number;
      generations: number;
      tokens: number;
      avgCostPerGeneration: number;
      percentage: number;          // % of total cost
    };
  };

  // Top N most expensive models
  topModels?: Array<{
    provider: string;
    model: string;
    cost: number;
    generations: number;
    avgCostPerGeneration: number;
    percentage: number;
  }>;

  // Cost breakdown by user
  costByUser?: {
    [userId: string]: {
      userName?: string;
      cost: number;
      generations: number;
      percentage: number;
    };
  };

  // Cost breakdown by room
  costByRoom?: {
    [roomId: string]: {
      roomName?: string;
      cost: number;
      generations: number;
      percentage: number;
    };
  };

  // Cost breakdown by purpose
  costByPurpose?: {
    [purpose: string]: {
      cost: number;
      generations: number;
      percentage: number;
    };
  };

  // Time-series data for graphing
  timeSeries?: Array<{
    timestamp: string;             // ISO timestamp
    cost: number;                  // Cost during this interval
    generations: number;           // Generations during this interval
    tokens: number;                // Tokens during this interval
    avgResponseTime: number;       // Avg response time during this interval
  }>;

  // Latency metrics
  latency?: {
    avgLatency: number;            // Average latency in ms
    minLatency: number;            // Minimum latency in ms
    maxLatency: number;            // Maximum latency in ms
    p50: number;                   // 50th percentile (median)
    p95: number;                   // 95th percentile
    p99: number;                   // 99th percentile
  };

  // Token usage metrics
  tokenMetrics?: {
    avgInputTokens: number;
    avgOutputTokens: number;
    inputOutputRatio: number;      // ratio of input to output tokens
    tokensPerSecond: number;       // Overall throughput
  };

  // Cost efficiency metrics
  efficiency?: {
    costPerToken: number;          // Average cost per token
    costPerSecond: number;         // Average cost per second of response time
    tokensPerDollar: number;       // How many tokens per $1
  };
}

/**
 * AICost — Type-safe command executor
 *
 * Usage:
 *   import { AICost } from '...shared/AICostTypes';
 *   const result = await AICost.execute({ ... });
 */
export const AICost = {
  execute(params: CommandInput<AICostParams>): Promise<AICostResult> {
    return Commands.execute<AICostParams, AICostResult>('ai/cost', params as Partial<AICostParams>);
  },
  commandName: 'ai/cost' as const,
} as const;
