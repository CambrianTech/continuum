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
  'candle': {
    provider: 'candle',
    model: LOCAL_MODELS.DEFAULT,  // Must match CandleAdapter default_model
    temperature: 0.7,
    maxTokens: 200,
    // Context window is defined in ModelContextWindows.ts (SINGLE SOURCE OF TRUTH)
    // ChatRAGBuilder uses ModelContextWindows.getContextWindow(modelId) for budget calculation
    // Latency-aware budgeting further limits slow local models to prevent timeouts
    systemPrompt: 'You are a helpful local AI assistant powered by Candle inference. You provide fast, privacy-preserving responses.'
  },
  'groq': {
    provider: 'groq',
    model: 'llama-3.1-8b-instant',
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
    model: 'meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo',
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
 * Get model configuration for a provider
 * Falls back to candle if provider not found
 */
export function getModelConfigForProvider(provider: string): ModelConfig {
  const baseConfig = DEFAULT_MODEL_CONFIGS[provider] || DEFAULT_MODEL_CONFIGS['candle'];

  // Add SOTA capability to cloud providers
  if (SOTA_PROVIDERS.has(provider)) {
    return {
      ...baseConfig,
      capabilities: ['sota']
    };
  }

  return baseConfig;
}
