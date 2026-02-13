/**
 * PricingConfig - Single source of truth for AI model pricing
 *
 * ALL model pricing should be defined here, not scattered across files.
 * When adding new models/providers, update THIS file only.
 *
 * Prices are per 1 MILLION tokens (industry standard).
 * Local models (Ollama, Candle) are free.
 *
 * TODO: Make this API-driven when providers expose pricing endpoints.
 */

export interface ModelPricing {
  /** Cost per 1M input tokens (USD) */
  inputPerMillion: number;
  /** Cost per 1M output tokens (USD) */
  outputPerMillion: number;
}

/**
 * Pricing database - keyed by provider/model
 * Wildcard patterns supported: provider/* matches all models for a provider
 */
const PRICING_DB: Record<string, ModelPricing> = {
  // ====== Anthropic ======
  'anthropic/claude-3-opus': { inputPerMillion: 15.0, outputPerMillion: 75.0 },
  'anthropic/claude-3-sonnet': { inputPerMillion: 3.0, outputPerMillion: 15.0 },
  'anthropic/claude-3-haiku': { inputPerMillion: 0.25, outputPerMillion: 1.25 },
  'anthropic/claude-3.5-sonnet': { inputPerMillion: 3.0, outputPerMillion: 15.0 },
  'anthropic/claude-3.5-haiku': { inputPerMillion: 0.80, outputPerMillion: 4.0 },
  'anthropic/claude-opus-4': { inputPerMillion: 15.0, outputPerMillion: 75.0 },

  // ====== OpenAI ======
  'openai/gpt-4o': { inputPerMillion: 5.0, outputPerMillion: 15.0 },
  'openai/gpt-4o-mini': { inputPerMillion: 0.15, outputPerMillion: 0.6 },
  'openai/gpt-4-turbo': { inputPerMillion: 10.0, outputPerMillion: 30.0 },
  'openai/gpt-3.5-turbo': { inputPerMillion: 0.5, outputPerMillion: 1.5 },
  'openai/o1': { inputPerMillion: 15.0, outputPerMillion: 60.0 },
  'openai/o1-mini': { inputPerMillion: 3.0, outputPerMillion: 12.0 },

  // ====== Together AI ======
  'together/llama-3': { inputPerMillion: 0.2, outputPerMillion: 0.2 },
  'together/llama-3.1-70b': { inputPerMillion: 0.88, outputPerMillion: 0.88 },
  'together/llama-3.1-8b': { inputPerMillion: 0.18, outputPerMillion: 0.18 },
  'together/mixtral-8x7b': { inputPerMillion: 0.6, outputPerMillion: 0.6 },
  'together/*': { inputPerMillion: 0.5, outputPerMillion: 0.5 }, // Default for Together

  // ====== DeepSeek ======
  'deepseek/deepseek-chat': { inputPerMillion: 0.14, outputPerMillion: 0.28 },
  'deepseek/deepseek-coder': { inputPerMillion: 0.14, outputPerMillion: 0.28 },
  'deepseek/*': { inputPerMillion: 0.14, outputPerMillion: 0.28 }, // Default for DeepSeek

  // ====== Groq (heavily rate limited, effectively free tier) ======
  'groq/llama-3': { inputPerMillion: 0.05, outputPerMillion: 0.08 },
  'groq/mixtral-8x7b': { inputPerMillion: 0.27, outputPerMillion: 0.27 },
  'groq/*': { inputPerMillion: 0.1, outputPerMillion: 0.1 }, // Default for Groq

  // ====== Fireworks ======
  'fireworks/llama-v3p1-70b': { inputPerMillion: 0.9, outputPerMillion: 0.9 },
  'fireworks/llama-v3p1-8b': { inputPerMillion: 0.2, outputPerMillion: 0.2 },
  'fireworks/*': { inputPerMillion: 0.5, outputPerMillion: 0.5 }, // Default for Fireworks

  // ====== xAI (Grok) ======
  'xai/grok-beta': { inputPerMillion: 5.0, outputPerMillion: 15.0 },
  'xai/*': { inputPerMillion: 5.0, outputPerMillion: 15.0 }, // Default for xAI

  // ====== Local (FREE) ======
  'candle/*': { inputPerMillion: 0, outputPerMillion: 0 },
  'candle-grpc/*': { inputPerMillion: 0, outputPerMillion: 0 },
  'sentinel/*': { inputPerMillion: 0, outputPerMillion: 0 },
};

// Default pricing for unknown providers (assume it costs something)
const DEFAULT_PRICING: ModelPricing = { inputPerMillion: 0, outputPerMillion: 0 };

/**
 * Get pricing for a specific provider/model combination.
 * Falls back to provider wildcard, then default.
 *
 * @param provider - Provider ID (e.g., 'anthropic', 'openai')
 * @param model - Model ID (e.g., 'claude-3-opus', 'gpt-4o')
 * @returns Pricing per million tokens
 */
export function getModelPricing(provider: string, model: string): ModelPricing {
  // Try exact match first
  const exactKey = `${provider}/${model}`;
  if (PRICING_DB[exactKey]) {
    return PRICING_DB[exactKey];
  }

  // Try provider wildcard
  const wildcardKey = `${provider}/*`;
  if (PRICING_DB[wildcardKey]) {
    return PRICING_DB[wildcardKey];
  }

  // Unknown provider/model - return default (free)
  return DEFAULT_PRICING;
}

/**
 * Calculate total cost for a request.
 *
 * @param provider - Provider ID
 * @param model - Model ID
 * @param inputTokens - Number of input tokens
 * @param outputTokens - Number of output tokens
 * @returns Total cost in USD
 */
export function calculateCost(
  provider: string,
  model: string,
  inputTokens: number,
  outputTokens: number
): number {
  const pricing = getModelPricing(provider, model);
  const inputCost = (inputTokens / 1_000_000) * pricing.inputPerMillion;
  const outputCost = (outputTokens / 1_000_000) * pricing.outputPerMillion;
  return inputCost + outputCost;
}

/**
 * Check if a provider is free (local inference).
 */
export function isProviderFree(provider: string): boolean {
  const pricing = getModelPricing(provider, '*');
  return pricing.inputPerMillion === 0 && pricing.outputPerMillion === 0;
}
