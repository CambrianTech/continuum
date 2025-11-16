/**
 * Persona Model Configurations
 *
 * Default model configurations for each AI provider.
 * Extracted from PersonaUser.ts for better organization and maintainability.
 */

import type { ModelConfig } from '../../../../commands/user/create/shared/UserCreateTypes';

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
  'xai'
]);

/**
 * Default model configurations by provider
 */
export const DEFAULT_MODEL_CONFIGS: Record<string, ModelConfig> = {
  'ollama': {
    provider: 'ollama',
    model: 'llama3.2:3b',
    temperature: 0.7,
    maxTokens: 150,
    systemPrompt: 'You are Local Assistant, running privately on this machine via Ollama. You provide helpful responses while keeping all data local and private.'
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
    model: 'claude-3-5-sonnet-20241022', // Latest Claude 3.5 Sonnet (Oct 2024)
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
    model: 'accounts/fireworks/models/llama-v3p1-8b-instruct', // Default from FireworksBaseConfig
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
  }
};

/**
 * Get model configuration for a provider
 * Falls back to ollama if provider not found
 */
export function getModelConfigForProvider(provider: string): ModelConfig {
  const baseConfig = DEFAULT_MODEL_CONFIGS[provider] || DEFAULT_MODEL_CONFIGS['ollama'];

  // Add SOTA capability to cloud providers
  if (SOTA_PROVIDERS.has(provider)) {
    return {
      ...baseConfig,
      capabilities: ['sota']
    };
  }

  return baseConfig;
}
