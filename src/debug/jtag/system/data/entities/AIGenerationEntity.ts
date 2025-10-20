/**
 * AIGenerationEntity - Persistent tracking of AI generations for cost/usage analytics
 *
 * Stores every AI text generation with tokens, cost, and metadata for:
 * - Cost tracking over time
 * - Usage analytics (tokens/sec, latency)
 * - Model performance comparison
 * - Provider cost breakdown
 */

import { BaseEntity } from './BaseEntity';
import type { UUID } from '../../core/types/CrossPlatformUUID';

export interface AIGenerationData {
  id: UUID;

  // Generation metadata
  timestamp: number;
  requestId: string;

  // AI model info
  provider: string;              // 'openai', 'anthropic', 'ollama', etc.
  model: string;                 // 'gpt-4', 'claude-3-opus', 'deepseek-r1', etc.

  // Usage metrics
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  estimatedCost: number;         // USD
  responseTime: number;          // milliseconds

  // Context
  userId?: UUID;                 // PersonaUser or HumanUser who requested
  roomId?: UUID;                 // Chat room context (if applicable)
  purpose?: string;              // 'chat', 'should-respond', 'generate-response', etc.

  // Result
  finishReason: 'stop' | 'length' | 'error';
  success: boolean;
  error?: string;

  // Versioning
  version: number;
  createdAt: number;
  updatedAt: number;
}

export class AIGenerationEntity extends BaseEntity<AIGenerationData> {
  protected getCollectionName(): string {
    return 'ai_generations';
  }

  /**
   * Create a new AI generation record from TextGenerationResponse
   */
  static async createFromResponse(
    response: {
      text: string;
      finishReason: 'stop' | 'length' | 'error';
      model: string;
      provider: string;
      usage: {
        inputTokens: number;
        outputTokens: number;
        totalTokens: number;
        estimatedCost?: number;
      };
      responseTime: number;
      requestId: string;
      error?: string;
    },
    context: {
      userId?: UUID;
      roomId?: UUID;
      purpose?: string;
    }
  ): Promise<AIGenerationEntity> {
    const now = Date.now();
    const entity = new AIGenerationEntity();

    entity.data = {
      id: entity.generateId(),
      timestamp: now,
      requestId: response.requestId,
      provider: response.provider,
      model: response.model,
      inputTokens: response.usage.inputTokens,
      outputTokens: response.usage.outputTokens,
      totalTokens: response.usage.totalTokens,
      estimatedCost: response.usage.estimatedCost || 0,
      responseTime: response.responseTime,
      userId: context.userId,
      roomId: context.roomId,
      purpose: context.purpose,
      finishReason: response.finishReason,
      success: !response.error,
      error: response.error,
      version: 1,
      createdAt: now,
      updatedAt: now
    };

    return entity;
  }

  /**
   * Query cost aggregation for time period
   */
  static async getTotalCost(startTime: number, endTime: number): Promise<number> {
    // TODO: Implement data/query with aggregation
    // For now, return placeholder
    return 0;
  }

  /**
   * Query cost breakdown by provider
   */
  static async getCostByProvider(startTime: number, endTime: number): Promise<Record<string, number>> {
    // TODO: Implement data/query with groupBy
    return {};
  }

  /**
   * Query token usage over time
   */
  static async getTokenUsage(startTime: number, endTime: number, intervalMs: number): Promise<Array<{
    timestamp: number;
    totalTokens: number;
    inputTokens: number;
    outputTokens: number;
  }>> {
    // TODO: Implement time-series aggregation
    return [];
  }

  /**
   * Query request count over time
   */
  static async getRequestCount(startTime: number, endTime: number, intervalMs: number): Promise<Array<{
    timestamp: number;
    count: number;
  }>> {
    // TODO: Implement time-series aggregation
    return [];
  }

  /**
   * Query latency metrics
   */
  static async getLatencyMetrics(startTime: number, endTime: number): Promise<{
    avgLatency: number;
    minLatency: number;
    maxLatency: number;
    p50: number;
    p95: number;
    p99: number;
  }> {
    // TODO: Implement aggregation with percentiles
    return {
      avgLatency: 0,
      minLatency: 0,
      maxLatency: 0,
      p50: 0,
      p95: 0,
      p99: 0
    };
  }
}
