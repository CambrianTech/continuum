/**
 * Adapter Types - Re-exports from unified AI type system
 *
 * Wire types come from Rust (via ts-rs). This file re-exports them
 * plus defines adapter-specific types not in the wire protocol.
 */

// Re-export wire types from unified source
export type { ModelCapability, ModelInfo, HealthStatus } from '../../AIProviderTypesV2';
export type { TextGenerationRequest, TextGenerationResponse, UsageMetrics } from '../../AIProviderTypesV2';

/**
 * Model capability profile (adapter-specific, not a wire type)
 */
export interface ModelCapabilities {
  readonly modelId: string;
  readonly providerId: string;
  readonly capabilities: import('../../../../../shared/generated/ai').ModelCapability[];
  readonly maxContextTokens: number;
  readonly supportsImages: boolean;
  readonly supportsToolUse: boolean;
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
 * Model recommendation
 */
export interface ModelRecommendation {
  readonly modelId: string;
  readonly name: string;
  readonly description: string;
  readonly size: string;
  readonly quality: 'excellent' | 'good' | 'fair';
  readonly speed: 'fast' | 'medium' | 'slow';
  readonly free: boolean;
  readonly requiresAPIKey: boolean;
  readonly capabilities: import('../../../../../shared/generated/ai').ModelCapability[];
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
