/**
 * Persona Model Configurations
 *
 * Default model configurations for each AI provider.
 * Extracted from PersonaUser.ts for better organization and maintainability.
 */

import type { ModelConfig } from '../../../data/entities/UserEntity';
import { MODEL_IDS, LOCAL_MODELS } from '../../../shared/Constants';

/**
 * SOTA (State-of-the-Art) Providers
 * Cloud providers with advanced capabilities beyond local models
 */
export const SOTA_PROVIDERS = new Set([
  'groq',
  'deepseek',
  'anthropic',
  'openai',
  'together',
  'fireworks',
  'xai',
  'google',
  'alibaba'
]);

/**
 * Default model configurations by provider
 */
export const DEFAULT_MODEL_CONFIGS: Record<string, ModelConfig> = {
  // 'local' = GPU-auto-routed. The Rust AdapterRegistry picks the best
  // available GPU adapter (DMR with Metal/CUDA, or llama-vulkan) based
  // on what's installed. The 'local' provider name is treated as "auto-
  // select" in adapter.rs select() — it drops through to device-filtered
  // priority-order selection instead of pinning a specific adapter.
  'local': {
    provider: 'local',
    model: LOCAL_MODELS.DEFAULT,
    temperature: 0.7,
    // 2500 — local default model is qwen3.5-4b-code-forged, a REASONING
    // model that emits 500-800 tokens of <think>...</think> before the
    // visible response. 1000 cut the model off mid-reasoning, leaving
    // 200-500 for the actual reply (often cut off entirely; visible as
    // "Thinking Process: 1. Analyze..." truncated in chat). 2500 fits
    // both phases: reasoning preamble (~15s) + visible response (~10-30s)
    // at ~50 tok/s on Mac Metal. Preserves the smart-AND-fast property —
    // we forged this model specifically because it reasons.
    maxTokens: 2500,
    systemPrompt: 'You are a helpful AI assistant running locally via Continuum. You provide thoughtful, concise responses.'
  },
  // 'candle' was removed as an inference adapter. The entry is GONE — any
  // lookup for 'candle' should fall through to 'local' at the call site.
  // Anyone seeing a missing-key error here should change their persona's
  // modelConfig.provider from 'candle' to 'local' (DB-side fix), not
  // re-add this entry.
  'groq': {
    provider: 'groq',
    model: 'llama-3.3-70b-versatile',
    temperature: 0.8,
    maxTokens: 2000,
    systemPrompt: 'You are Groq Lightning, powered by ultra-fast LPU inference. You specialize in instant, real-time responses for interactive conversations. Keep responses concise and engaging.'
  },
  'deepseek': {
    provider: 'deepseek',
    model: 'deepseek-chat',
    temperature: 0.7,
    maxTokens: 2000,
    systemPrompt: 'You are DeepSeek Assistant, powered by cost-effective SOTA models. You provide high-quality technical assistance with efficient reasoning and clear explanations.'
  },
  'anthropic': {
    provider: 'anthropic',
    model: MODEL_IDS.ANTHROPIC.SONNET_4_5,
    temperature: 0.7,
    maxTokens: 2000,
    systemPrompt: 'You are a helpful AI assistant powered by Anthropic Claude. You provide thoughtful, detailed responses with careful reasoning and helpful explanations.'
  },
  'openai': {
    provider: 'openai',
    model: 'gpt-4',
    temperature: 0.7,
    maxTokens: 3000,
    systemPrompt: 'You are an OpenAI GPT-4 assistant. You provide comprehensive, well-reasoned responses with balanced perspectives and clear communication.'
  },
  'together': {
    provider: 'together',
    model: 'meta-llama/Meta-Llama-3.1-8B-Instruct-Turbo',
    temperature: 0.7,
    maxTokens: 2000,
    systemPrompt: 'You are a helpful AI assistant powered by Together.ai. You provide efficient, well-structured responses with clear reasoning.'
  },
  'fireworks': {
    provider: 'fireworks',
    model: 'accounts/fireworks/models/llama-v3p3-70b-instruct', // Llama 3.3 70B - more reliable than deprecated 3.1 8B
    temperature: 0.7,
    maxTokens: 2000,
    systemPrompt: 'You are Fireworks AI assistant. You provide fast, high-quality responses optimized for production workloads.'
  },
  'xai': {
    provider: 'xai',
    model: 'grok-4',
    temperature: 0.8,
    maxTokens: 2000,
    systemPrompt: 'You are Grok, powered by xAI. You provide direct, intelligent responses with a focus on truth-seeking and helpful information.'
  },
  'sentinel': {
    provider: 'sentinel',
    model: 'gpt2',
    temperature: 0.7,
    maxTokens: 150,
    contextWindow: 1024,  // GPT-2 max context length
    systemPrompt: 'You are Sentinel, powered by local pre-trained models from the Sentinel-AI model zoo. You provide helpful responses while keeping all data local and private.',
    promptFormat: 'base'  // GPT-2 is a base model, needs "User: ...\n\nAssistant:" format
  },
  'google': {
    provider: 'google',
    model: 'gemini-2.0-flash',
    temperature: 0.7,
    maxTokens: 2000,
    systemPrompt: 'You are Gemini, powered by Google AI. You provide helpful, accurate responses with access to a broad knowledge base.'
  },
  'alibaba': {
    provider: 'alibaba',
    model: 'qwen3-omni-flash-realtime',
    temperature: 0.7,
    maxTokens: 2000,
    systemPrompt: 'You are Qwen3-Omni, powered by Alibaba Cloud. You provide multimodal assistance with vision, audio, and text capabilities.'
  },
};

/**
 * Get model configuration for a provider.
 * Throws if provider has no config — every provider must be registered.
 */
export function getModelConfigForProvider(provider: string): ModelConfig {
  const baseConfig = DEFAULT_MODEL_CONFIGS[provider];
  if (!baseConfig) {
    throw new Error(`No model config for provider '${provider}'. Add it to DEFAULT_MODEL_CONFIGS.`);
  }

  // Add SOTA capability to cloud providers
  if (SOTA_PROVIDERS.has(provider)) {
    return {
      ...baseConfig,
      capabilities: ['sota']
    };
  }

  return baseConfig;
}
