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
 */

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

  // Anthropic Models (Claude)
  'claude-3-opus': 200000,
  'claude-3-sonnet': 200000,
  'claude-3-haiku': 200000,
  'claude-3-5-sonnet': 200000,
  'claude-3-5-haiku': 200000,
  'claude-opus-4': 200000,

  // Meta Models (Llama) via Ollama
  'llama3.2': 128000,
  'llama3.2:3b': 128000,
  'llama3.2:1b': 128000,
  'llama3.1': 128000,
  'llama3.1:70b': 128000,
  'llama3.1:8b': 128000,

  // Qwen Models via Ollama
  'qwen2.5': 128000,
  'qwen2.5:7b': 128000,
  'qwen2.5:14b': 128000,
  'qwen2.5:32b': 128000,
  'qwen2.5:72b': 128000,
  'qwq': 128000,  // Qwen reasoning model

  // Google Models
  'gemini-pro': 32768,
  'gemini-1.5-pro': 1000000,
  'gemini-1.5-flash': 1000000,

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
};

/**
 * Default context window for unknown models
 * Using a conservative 8K to avoid overflow
 */
export const DEFAULT_CONTEXT_WINDOW = 8192;

/**
 * Get context window size for a model
 *
 * Supports:
 * - Direct model name lookup (e.g., "gpt-4")
 * - Versioned model lookup (e.g., "llama3.2:3b" → "llama3.2")
 * - Prefix matching for similar models
 *
 * @param model - Model identifier (e.g., "gpt-4", "claude-3-sonnet")
 * @returns Context window size in tokens, or DEFAULT_CONTEXT_WINDOW if model not found
 */
export function getContextWindow(model: string): number {
  // Direct match
  if (MODEL_CONTEXT_WINDOWS[model]) {
    return MODEL_CONTEXT_WINDOWS[model];
  }

  // Try without version suffix (e.g., 'llama3.2:3b' → 'llama3.2')
  const baseModel = model.split(':')[0];
  if (MODEL_CONTEXT_WINDOWS[baseModel]) {
    return MODEL_CONTEXT_WINDOWS[baseModel];
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
export function isLargeContextModel(modelId: string): boolean {
  return getContextWindow(modelId) > 32768;
}

/**
 * Get recommended output token budget for a model
 * Typically 10-25% of context window, capped at 4K for most use cases
 */
export function getRecommendedMaxOutputTokens(modelId: string): number {
  const contextWindow = getContextWindow(modelId);

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
  safetyMargin: number = 500
): number {
  const contextWindow = getContextWindow(modelId);
  return Math.max(0, contextWindow - reservedOutputTokens - safetyMargin);
}
