/**
 * ModelTiers - Semantic model naming system
 *
 * Purpose: Abstract away provider-specific model version strings with semantic tiers.
 *
 * User Requirements (from conversation):
 * - Ship first with "free" options demonstrable on variety of computers
 * - Primary target: M1 Pro (Apple Silicon)
 * - Eventually support "best mode" (high-end hardware)
 * - Semantic tags: "fast", "free", "balanced"
 * - Bidirectional mapping: tier → model ID AND model ID → tier
 *
 * Design:
 * - Single enum for core tiers (fast, balanced, premium, etc.)
 * - Tags interface for classification (cost, hardware, capabilities)
 * - Each adapter implements resolveModelTier() and classifyModel()
 * - PersonaUser configs use tiers instead of version strings
 */

/**
 * Core semantic model tiers
 *
 * Examples:
 * - FAST: claude-3-haiku, gpt-4o-mini, llama-3.1-8b
 * - BALANCED: claude-3-5-sonnet, gpt-4o, mixtral-8x7b
 * - PREMIUM: claude-3-opus, o1-pro, llama-3.1-405b
 * - FREE: candle models, free API tiers
 * - LATEST: Always newest version (e.g., claude-3-5-sonnet-20250122)
 */
export enum ModelTier {
  /** Fast and cheap models - low latency, lower capability */
  FAST = 'fast',

  /** Balanced performance/cost - production workload default */
  BALANCED = 'balanced',

  /** Most capable models - highest quality, higher cost */
  PREMIUM = 'premium',

  /** Always latest version - auto-updates when provider releases new models */
  LATEST = 'latest',

  /** Free tier models - no API cost (local or free tier) */
  FREE = 'free',
}

/**
 * Hardware optimization targets
 * Based on user requirement: "M1 Pro as initial target"
 */
export enum HardwareTarget {
  /** Apple Silicon (M1/M2/M3) - optimized for Metal/MLX */
  APPLE_SILICON = 'apple-silicon',

  /** NVIDIA GPUs - CUDA-optimized */
  NVIDIA_GPU = 'nvidia-gpu',

  /** General CPU inference - no special acceleration */
  GENERAL = 'general',
}

/**
 * Cost tier for filtering
 * User requirement: "free" dimension is critical
 */
export enum CostTier {
  /** Completely free (local models, free API tiers) */
  FREE = 'free',

  /** Low cost ($0.001-$0.01 per 1K tokens) */
  LOW = 'low',

  /** Medium cost ($0.01-$0.10 per 1K tokens) */
  MEDIUM = 'medium',

  /** High cost (>$0.10 per 1K tokens) */
  HIGH = 'high',
}

/**
 * Complete model classification
 * Bidirectional mapping requirement: turn "api results into these terms"
 */
export interface ModelTags {
  /** Semantic tier (fast/balanced/premium/etc.) */
  tier: ModelTier;

  /** Cost classification */
  costTier: CostTier;

  /** Hardware optimization (if applicable) */
  hardwareOptimized?: HardwareTarget;

  /** Provider identifier */
  provider: string;

  /** Actual model ID from provider API */
  actualModelId: string;

  /** Human-readable model name */
  displayName: string;

  /** Context window size */
  contextWindow: number;

  /** Pricing details */
  costPer1kTokens?: {
    input: number;
    output: number;
  };

  /** Capabilities */
  capabilities: string[];

  /** Is this a local model? */
  isLocal: boolean;
}

/**
 * Tier resolution result
 * Used when adapter resolves semantic tier → actual model ID
 */
export interface ModelResolution {
  /** Resolved model ID */
  modelId: string;

  /** Provider name */
  provider: string;

  /** Display name */
  displayName: string;

  /** Full classification */
  tags: ModelTags;
}

/**
 * Helper to determine cost tier from pricing
 */
export function calculateCostTier(costPer1kTokens?: { input: number; output: number }): CostTier {
  if (!costPer1kTokens || (costPer1kTokens.input === 0 && costPer1kTokens.output === 0)) {
    return CostTier.FREE;
  }

  const avgCost = (costPer1kTokens.input + costPer1kTokens.output) / 2;

  if (avgCost < 0.01) return CostTier.LOW;
  if (avgCost < 0.1) return CostTier.MEDIUM;
  return CostTier.HIGH;
}

/**
 * Helper to check if model is free
 */
export function isFreeModel(tags: ModelTags): boolean {
  return tags.costTier === CostTier.FREE;
}

/**
 * Helper to check if model is optimized for Apple Silicon
 */
export function isAppleSiliconOptimized(tags: ModelTags): boolean {
  return tags.hardwareOptimized === HardwareTarget.APPLE_SILICON;
}
