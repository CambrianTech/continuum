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
import { TextField, NumberField, BooleanField, Archive } from '../decorators/FieldDecorators';
import type { UUID } from '../../core/types/CrossPlatformUUID';

@Archive({
  sourceHandle: 'primary',
  destHandle: 'archive',
  maxRows: 50000,
  rowsPerArchive: 5000,
  maxArchiveFileRows: 500000,
  orderByField: 'createdAt'
})
export class AIGenerationEntity extends BaseEntity {
  // Single source of truth for collection name
  static readonly collection = 'ai_generations';

  // Generation metadata
  @NumberField()
  timestamp!: number;

  @TextField()
  requestId!: string;

  // AI model info
  @TextField()
  provider!: string;              // 'openai', 'anthropic', 'ollama', etc.

  @TextField()
  model!: string;                 // 'gpt-4', 'claude-3-opus', 'deepseek-r1', etc.

  // Usage metrics
  @NumberField()
  inputTokens!: number;

  @NumberField()
  outputTokens!: number;

  @NumberField()
  totalTokens!: number;

  @NumberField()
  estimatedCost!: number;         // USD

  @NumberField()
  responseTime!: number;          // milliseconds

  // Context (optional)
  @TextField({ nullable: true })
  userId?: UUID;                 // PersonaUser or HumanUser who requested

  @TextField({ nullable: true })
  roomId?: UUID;                 // Chat room context (if applicable)

  @TextField({ nullable: true })
  purpose?: string;              // 'chat', 'should-respond', 'generate-response', etc.

  // Result
  @TextField()
  finishReason!: 'stop' | 'length' | 'error' | 'tool_use';

  @BooleanField()
  success!: boolean;

  @TextField({ nullable: true })
  error?: string;

  get collection(): string {
    return 'ai_generations';
  }

  validate(): { success: boolean; error?: string } {
    if (!this.requestId || !this.provider || !this.model) {
      return { success: false, error: 'Missing required fields: requestId, provider, model' };
    }

    if (this.inputTokens < 0 || this.outputTokens < 0) {
      return { success: false, error: 'Token counts cannot be negative' };
    }

    if (this.estimatedCost < 0) {
      return { success: false, error: 'Cost cannot be negative' };
    }

    return { success: true };
  }

  /**
   * Create a new AI generation record from TextGenerationResponse
   */
  static async createFromResponse(
    response: {
      text: string;
      finishReason: 'stop' | 'length' | 'error' | 'tool_use';
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

    const result = AIGenerationEntity.create({
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
      error: response.error
    });

    if (!result.success || !result.entity) {
      throw new Error(`Failed to create AIGenerationEntity: ${result.error}`);
    }

    return result.entity;
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
