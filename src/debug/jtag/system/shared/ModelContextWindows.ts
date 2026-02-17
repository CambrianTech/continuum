/**
 * Model Context Windows - Centralized Configuration (SINGLE SOURCE OF TRUTH)
 *
 * This file is the ONLY place model context window sizes should be defined.
 * When adding new models, update THIS FILE ONLY.
 *
 * Used by:
 * - ChatRAGBuilder (message count budgeting)
 * - RAGBudgetServerCommand (token budget calculation)
 * - PersonaUser (model capability checks)
 *
 * Dynamic discovery:
 * ModelRegistry (populated async from provider APIs in initializeDeferred)
 * is checked FIRST. Static maps below are the fallback when the registry
 * hasn't discovered a model yet or the provider API is unavailable.
 *
 * Provider-scoped lookups:
 * All functions accept an optional `provider` parameter. When provided,
 * ModelRegistry returns only that provider's entry — preventing collisions
 * where the same modelId exists on multiple providers with different context
 * windows (e.g., meta-llama/Llama-3.1-8B-Instruct: 131K on Together, 1400 on Candle).
 */

import { ModelRegistry } from './ModelRegistry';

/** Known local provider names for inference speed classification */
const LOCAL_PROVIDERS = new Set(['candle', 'sentinel']);

/**
 * Model context windows in tokens
 *
 * @remarks
 * Context windows determine how much conversation history can fit in a single request.
 * Larger context windows allow more messages but cost more and may be slower.
 *
 * Default: 8192 tokens (8K) if model not found
 */
export const MODEL_CONTEXT_WINDOWS: Readonly<Record<string, number>> = {
  // OpenAI Models
  'gpt-3.5-turbo': 16385,
  'gpt-4': 8192,
  'gpt-4-turbo': 128000,
  'gpt-4o': 128000,
  'gpt-4o-mini': 128000,
  'o1': 200000,
  'o1-mini': 128000,

  // Anthropic Models (Claude) — versioned IDs used at runtime
  'claude-sonnet-4-5-20250929': 200000,  // MODEL_IDS.ANTHROPIC.SONNET_4_5
  'claude-opus-4-20250514': 200000,      // MODEL_IDS.ANTHROPIC.OPUS_4
  'claude-3-5-haiku-20241022': 200000,   // MODEL_IDS.ANTHROPIC.HAIKU_3_5
  'claude-sonnet-4': 200000,             // Alias used in UserDataSeed
  'claude-sonnet-4-5': 200000,           // Date-stripped alias
  // Legacy naming (kept for backward compatibility)
  'claude-3-opus': 200000,
  'claude-3-sonnet': 200000,
  'claude-3-haiku': 200000,
  'claude-3-5-sonnet': 200000,
  'claude-3-5-haiku': 200000,
  'claude-opus-4': 200000,

  // Meta Models (Llama) — cloud API naming (dashes)
  'llama-3.1-8b-instant': 131072,                           // Groq LPU
  'meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo': 131072,  // Together.ai
  'accounts/fireworks/models/llama-v3p1-8b-instruct': 131072,  // Fireworks.ai (deprecated)
  'accounts/fireworks/models/llama-v3p3-70b-instruct': 131072,  // Fireworks.ai Llama 3.3 70B
  // Meta Models (Llama) — legacy short names
  'llama3.2': 128000,
  'llama3.2:3b': 128000,
  'llama3.2:1b': 128000,
  'llama3.1': 128000,
  'llama3.1:70b': 128000,
  'llama3.1:8b': 128000,

  // HuggingFace IDs (Candle adapter) — FALLBACK only.
  // Source of truth is CandleAdapter.capabilities().max_context_window in Rust,
  // which feeds into ModelRegistry at startup via registerLocalModels().
  // Candle quantized attention breaks at ~1000 input tokens on Metal.
  // See: https://github.com/huggingface/candle/issues/1566
  'meta-llama/Llama-3.1-8B-Instruct': 1400,
  'unsloth/Llama-3.2-3B-Instruct': 1400,
  'Qwen/Qwen2-1.5B-Instruct': 1400,
  'Qwen/Qwen2-0.5B-Instruct': 1400,

  // Qwen Models — legacy short names
  'qwen2.5': 128000,
  'qwen2.5:7b': 128000,
  'qwen2.5:14b': 128000,
  'qwen2.5:32b': 128000,
  'qwen2.5:72b': 128000,
  'qwq': 128000,  // Qwen reasoning model
  'qwen3-omni-flash-realtime': 128000,  // Alibaba Qwen 3 Omni

  // Google Models
  'gemini-pro': 32768,
  'gemini-1.5-pro': 1000000,
  'gemini-1.5-flash': 1000000,
  'gemini-2.0-flash': 1048576,  // Gemini 2.0 Flash

  // Mistral Models
  'mistral': 32768,
  'mistral:7b': 32768,
  'mixtral': 32768,
  'mistral-large': 128000,

  // DeepSeek Models
  'deepseek-coder': 128000,
  'deepseek-coder:6.7b': 16000,
  'deepseek-chat': 64000,
  'deepseek-r1': 128000,

  // Cohere Models
  'command-r': 128000,
  'command-r-plus': 128000,

  // X.AI Models
  'grok-3': 131072,
  'grok-4': 131072,
};

/**
 * Default context window for unknown models
 * Using a conservative 8K to avoid overflow
 */
export const DEFAULT_CONTEXT_WINDOW = 8192;

/**
 * Model inference speeds in tokens per second (TPS)
 *
 * These estimates are for INPUT token processing (prompt evaluation).
 * Used for latency-aware budgeting to prevent timeouts on slow models.
 *
 * Categories:
 * - Cloud APIs: ~1000+ TPS (fast, network-bound)
 * - Local large models (70B+): ~20-50 TPS
 * - Local medium models (7-14B): ~50-150 TPS
 * - Local small models (1-3B): ~100-200 TPS on Apple Silicon
 *
 * Conservative estimates - actual speed varies by hardware.
 */
export const MODEL_INFERENCE_SPEEDS: Readonly<Record<string, number>> = {
  // Cloud APIs - fast (network-bound, not compute-bound)
  'gpt-4': 1000,
  'gpt-4-turbo': 1000,
  'gpt-4o': 1000,
  'gpt-4o-mini': 1000,
  'claude-sonnet-4-5-20250929': 1000,
  'claude-opus-4-20250514': 1000,
  'claude-3-5-haiku-20241022': 1000,
  'claude-3-opus': 1000,
  'claude-3-sonnet': 1000,
  'claude-3-haiku': 1000,
  'claude-3-5-sonnet': 1000,
  'claude-opus-4': 1000,
  'llama-3.1-8b-instant': 1000,                            // Groq LPU
  'meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo': 1000,   // Together.ai
  'accounts/fireworks/models/llama-v3p1-8b-instruct': 1000, // Fireworks.ai
  'deepseek-chat': 1000,                                   // DeepSeek cloud
  'grok-3': 1000,                                          // xAI cloud
  'grok-4': 1000,                                          // xAI cloud
  'gemini-2.0-flash': 1000,                                // Google cloud
  'qwen3-omni-flash-realtime': 1000,                       // Alibaba cloud
  'gemini-pro': 1000,
  'gemini-1.5-pro': 1000,

  // Local small models via Candle (1-3B params)
  // ~100-200 TPS on Apple Silicon M1/M2
  'llama3.2:1b': 200,
  'llama3.2:3b': 100,  // Conservative for RAG-heavy prompts
  'qwen2.5:3b': 100,
  'qwen2:0.5b': 300,
  'qwen2:1.5b': 150,

  // HuggingFace IDs (used by Candle adapter directly)
  // These MUST match the actual model IDs used in Rust CandleAdapter
  'meta-llama/Llama-3.1-8B-Instruct': 40,   // Q4_K_M 8B - slower but much better quality
  'unsloth/Llama-3.2-3B-Instruct': 60,      // Q4_K_M quantized - conservative due to NaN/Inf at long contexts
  'Qwen/Qwen2-1.5B-Instruct': 80,
  'Qwen/Qwen2-0.5B-Instruct': 150,

  // Local medium models (7-14B params)
  // ~50-150 TPS on Apple Silicon
  'llama3.1:8b': 80,
  'llama3.2': 100,  // Default for llama3.2 family
  'qwen2.5:7b': 80,
  'qwen2.5:14b': 50,
  'mistral:7b': 80,

  // Local large models (30B+ params)
  // ~20-50 TPS on Apple Silicon
  'llama3.1:70b': 25,
  'qwen2.5:32b': 35,
  'qwen2.5:72b': 20,
  'mixtral': 40,

  // DeepSeek models
  'deepseek-coder:6.7b': 80,
  'deepseek-r1': 50,
};

/**
 * Default inference speed for unknown models (conservative)
 * Assumes local medium model speed
 */
export const DEFAULT_INFERENCE_SPEED = 80;

/**
 * Default target latency in seconds for inference
 * Used when calculating latency-aware token budgets
 */
export const DEFAULT_TARGET_LATENCY_SECONDS = 30;

/**
 * Get inference speed for a model in tokens per second.
 *
 * When provider is specified and the model is found in the registry:
 *   - Local providers (candle/sentinel): fall through to static speed map
 *   - Cloud providers: return 1000 TPS (network-bound)
 *
 * Bug fix: Previously, any registry hit assumed cloud (1000 TPS), even for
 * Candle models registered at 40 TPS. Now checks provider to classify correctly.
 */
export function getInferenceSpeed(model: string, provider?: string): number {
  const registry = ModelRegistry.sharedInstance();
  const discovered = registry.get(model, provider);
  if (discovered) {
    // Check if this is a local provider — don't assume cloud speed
    if (LOCAL_PROVIDERS.has(discovered.provider)) {
      // Fall through to static speed map for local providers
      // (registry has context windows, not inference speeds)
    } else {
      // Cloud APIs are ~1000 TPS (network-bound)
      return 1000;
    }
  }

  // Direct match
  if (MODEL_INFERENCE_SPEEDS[model]) {
    return MODEL_INFERENCE_SPEEDS[model];
  }

  // Try without version suffix
  const baseModel = model.split(':')[0];
  if (MODEL_INFERENCE_SPEEDS[baseModel]) {
    return MODEL_INFERENCE_SPEEDS[baseModel];
  }

  // Strip date suffix (e.g., 'claude-sonnet-4-5-20250929' → 'claude-sonnet-4-5')
  const dateStripped = model.replace(/-\d{8}$/, '');
  if (dateStripped !== model && MODEL_INFERENCE_SPEEDS[dateStripped]) {
    return MODEL_INFERENCE_SPEEDS[dateStripped];
  }

  // Try prefix matching
  for (const [key, value] of Object.entries(MODEL_INFERENCE_SPEEDS)) {
    if (model.startsWith(key) || key.startsWith(model)) {
      return value;
    }
  }

  return DEFAULT_INFERENCE_SPEED;
}

/**
 * Calculate maximum input tokens based on target latency
 *
 * Formula: maxInputTokens = targetLatencySeconds × tokensPerSecond
 *
 * This is the LATENCY constraint - separate from the context window constraint.
 * The actual limit is MIN(contextWindow, latencyLimit).
 *
 * @param model - Model identifier
 * @param targetLatencySeconds - Target response time (default: 30s)
 * @param provider - Optional provider for scoped lookup
 * @returns Maximum input tokens to stay within latency target
 */
export function getLatencyAwareTokenLimit(
  model: string,
  targetLatencySeconds: number = DEFAULT_TARGET_LATENCY_SECONDS,
  provider?: string
): number {
  const tokensPerSecond = getInferenceSpeed(model, provider);
  return Math.floor(targetLatencySeconds * tokensPerSecond);
}

/**
 * Check if a model is a slow local model (needs latency-aware budgeting)
 */
export function isSlowLocalModel(model: string, provider?: string): boolean {
  const speed = getInferenceSpeed(model, provider);
  return speed < 500;  // Below 500 TPS = needs latency awareness
}

/**
 * Get context window size for a model
 *
 * Supports:
 * - Direct model name lookup (e.g., "gpt-4")
 * - Versioned model lookup (e.g., "llama3.2:3b" → "llama3.2")
 * - Prefix matching for similar models
 * - Provider-scoped lookup to avoid cross-provider collisions
 *
 * @param model - Model identifier (e.g., "gpt-4", "claude-3-sonnet")
 * @param provider - Optional provider for scoped registry lookup
 * @returns Context window size in tokens, or DEFAULT_CONTEXT_WINDOW if model not found
 */
export function getContextWindow(model: string, provider?: string): number {
  // Check ModelRegistry first (live-discovered data from provider APIs)
  const discovered = ModelRegistry.sharedInstance().contextWindow(model, provider);
  if (discovered !== undefined) return discovered;

  // Direct match in static map
  if (MODEL_CONTEXT_WINDOWS[model]) {
    return MODEL_CONTEXT_WINDOWS[model];
  }

  // Try without version suffix (e.g., 'llama3.2:3b' → 'llama3.2')
  const baseModel = model.split(':')[0];
  if (MODEL_CONTEXT_WINDOWS[baseModel]) {
    return MODEL_CONTEXT_WINDOWS[baseModel];
  }

  // Strip date suffix (e.g., 'claude-sonnet-4-5-20250929' → 'claude-sonnet-4-5')
  const dateStripped = model.replace(/-\d{8}$/, '');
  if (dateStripped !== model && MODEL_CONTEXT_WINDOWS[dateStripped]) {
    return MODEL_CONTEXT_WINDOWS[dateStripped];
  }

  // Try prefix matching for versioned models
  for (const [key, value] of Object.entries(MODEL_CONTEXT_WINDOWS)) {
    if (model.startsWith(key) || key.startsWith(model)) {
      return value;
    }
  }

  return DEFAULT_CONTEXT_WINDOW;
}

/**
 * Check if a model has a large context window (>32K)
 * Useful for deciding whether to include more context in RAG
 */
export function isLargeContextModel(modelId: string, provider?: string): boolean {
  return getContextWindow(modelId, provider) > 32768;
}

/**
 * Get recommended output token budget for a model
 * Typically 10-25% of context window, capped at 4K for most use cases
 */
export function getRecommendedMaxOutputTokens(modelId: string, provider?: string): number {
  const contextWindow = getContextWindow(modelId, provider);

  // For small context windows, use 25%
  if (contextWindow <= 8192) {
    return Math.floor(contextWindow * 0.25);
  }

  // For large context windows, cap at 4K (most responses don't need more)
  return Math.min(4096, Math.floor(contextWindow * 0.1));
}

/**
 * Calculate available tokens for input given output reservation
 */
export function getAvailableInputTokens(
  modelId: string,
  reservedOutputTokens: number,
  safetyMargin: number = 500,
  provider?: string
): number {
  const contextWindow = getContextWindow(modelId, provider);
  return Math.max(0, contextWindow - reservedOutputTokens - safetyMargin);
}
