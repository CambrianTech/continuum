/**
 * Model Context Windows - Centralized configuration
 *
 * Single source of truth for model context window sizes.
 * Used by RAG commands and AI generation logic to calculate token budgets.
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
  // OpenAI GPT models
  'gpt-4': 8192,
  'gpt-4-turbo': 128000,
  'gpt-4o': 128000,
  'gpt-3.5-turbo': 16385,

  // Anthropic Claude models
  'claude-3-opus': 200000,
  'claude-3-sonnet': 200000,
  'claude-3-haiku': 200000,
  'claude-3-5-sonnet': 200000,

  // Local/Ollama models
  'llama3.2:3b': 128000,
  'llama3.1:70b': 128000,
  'deepseek-coder:6.7b': 16000,
  'qwen2.5:7b': 128000,
  'mistral:7b': 32768,

  // External API models
  'grok-3': 131072,  // Updated from grok-beta (deprecated 2025-09-15)
  'deepseek-chat': 64000
};

/**
 * Default context window for unknown models
 */
export const DEFAULT_CONTEXT_WINDOW = 8192;

/**
 * Get context window size for a model
 *
 * @param model - Model identifier (e.g., "gpt-4", "claude-3-sonnet")
 * @returns Context window size in tokens, or DEFAULT_CONTEXT_WINDOW if model not found
 */
export function getContextWindow(model: string): number {
  return MODEL_CONTEXT_WINDOWS[model] ?? DEFAULT_CONTEXT_WINDOW;
}
