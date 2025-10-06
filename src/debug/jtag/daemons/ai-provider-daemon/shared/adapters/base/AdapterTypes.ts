/**
 * Adapter Types - Shared interfaces for AI provider adapters
 *
 * Small, focused interfaces following single responsibility principle.
 * All providers implement these, no god objects.
 */

import type { UUID } from '../../../../../system/core/types/CrossPlatformUUID';

/**
 * Model capabilities
 */
export type ModelCapability =
  | 'text'
  | 'vision'
  | 'function-calling'
  | 'streaming'
  | 'embeddings'
  | 'multimodal';

/**
 * Model capability profile
 */
export interface ModelCapabilities {
  readonly modelId: string;
  readonly providerId: string;
  readonly capabilities: ModelCapability[];
  readonly maxContextTokens: number;
  readonly supportsImages: boolean;
  readonly supportsFunctionCalling: boolean;
  readonly supportsStreaming: boolean;
}

/**
 * Model installation progress
 */
export interface InstallProgress {
  readonly model: string;
  readonly status: 'downloading' | 'installing' | 'verifying' | 'complete' | 'failed';
  readonly bytesDownloaded: number;
  readonly bytesTotal: number;
  readonly percentComplete: number;
  readonly estimatedTimeRemaining?: number;
  readonly message?: string;
}

/**
 * Loaded model handle (reference to loaded model)
 */
export interface LoadedModelHandle {
  readonly modelId: string;
  readonly providerId: string;
  readonly loadedAt: number;
  readonly vramUsed?: number;
  readonly metadata?: Record<string, unknown>;
}

/**
 * Health status
 */
export interface HealthStatus {
  readonly status: 'healthy' | 'degraded' | 'unhealthy';
  readonly apiAvailable: boolean;
  readonly responseTime: number;
  readonly errorRate?: number;
  readonly lastChecked: number;
  readonly message?: string;
}

/**
 * Text generation request (provider-agnostic)
 */
export interface TextGenerationRequest {
  readonly messages: Array<{
    role: 'system' | 'user' | 'assistant';
    content: string;
    name?: string;
    images?: string[];  // Base64 or URLs
  }>;
  readonly model?: string;
  readonly temperature?: number;
  readonly maxTokens?: number;
  readonly systemPrompt?: string;
  readonly preferredProvider?: string;
  readonly requestId?: UUID;
}

/**
 * Text generation response (provider-agnostic)
 */
export interface TextGenerationResponse {
  readonly text: string;
  readonly finishReason: 'stop' | 'length' | 'error';
  readonly model: string;
  readonly provider: string;
  readonly usage?: {
    readonly inputTokens: number;
    readonly outputTokens: number;
    readonly totalTokens: number;
  };
  readonly responseTime: number;
  readonly requestId?: UUID;
}

/**
 * Model recommendation
 */
export interface ModelRecommendation {
  readonly modelId: string;
  readonly name: string;
  readonly description: string;
  readonly size: string;  // "2GB", "7GB"
  readonly quality: 'excellent' | 'good' | 'fair';
  readonly speed: 'fast' | 'medium' | 'slow';
  readonly free: boolean;
  readonly requiresAPIKey: boolean;
  readonly capabilities: ModelCapability[];
}

/**
 * Resource usage tracking
 */
export interface ResourceUsage {
  readonly vramUsed: number;
  readonly vramLimit: number;
  readonly loadedModels: number;
}

/**
 * Evicted model info
 */
export interface EvictedModel {
  readonly modelId: string;
  readonly vramFreed: number;
}

/**
 * Loaded model info (for LRU cache)
 */
export interface LoadedModelInfo {
  readonly vramSize: number;
  readonly loadedAt: number;
}
