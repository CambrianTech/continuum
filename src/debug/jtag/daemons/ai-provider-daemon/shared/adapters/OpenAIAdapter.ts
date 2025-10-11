/**
 * OpenAIAdapter - Official OpenAI API
 *
 * Supports:
 * - GPT-4, GPT-4 Turbo, GPT-3.5 Turbo
 * - DALL-E 3 image generation
 * - Text embeddings
 * - Multimodal (GPT-4V vision)
 *
 * Just 30 lines of code thanks to BaseOpenAICompatibleAdapter!
 */

import { BaseOpenAICompatibleAdapter } from './BaseOpenAICompatibleAdapter';
import type { ModelInfo } from '../AIProviderTypesV2';
import { getSecret } from '../../../../system/secrets/SecretManager';

export class OpenAIAdapter extends BaseOpenAICompatibleAdapter {
  constructor(apiKey?: string) {
    super({
      providerId: 'openai',
      providerName: 'OpenAI',
      apiKey: apiKey || getSecret('OPENAI_API_KEY', 'OpenAIAdapter') || '',
      baseUrl: 'https://api.openai.com',
      defaultModel: 'gpt-4-turbo',
      timeout: 60000,
      supportedCapabilities: [
        'text-generation',
        'chat',
        'image-generation',
        'image-analysis',
        'embeddings',
        'multimodal',
      ],
      models: [
        {
          id: 'gpt-4-turbo',
          name: 'GPT-4 Turbo',
          provider: 'openai',
          capabilities: ['text-generation', 'chat', 'multimodal'],
          contextWindow: 128000,
          maxOutputTokens: 4096,
          costPer1kTokens: { input: 0.01, output: 0.03 },
          supportsStreaming: true,
          supportsFunctions: true,
        },
        {
          id: 'gpt-4',
          name: 'GPT-4',
          provider: 'openai',
          capabilities: ['text-generation', 'chat'],
          contextWindow: 8192,
          maxOutputTokens: 4096,
          costPer1kTokens: { input: 0.03, output: 0.06 },
          supportsStreaming: true,
          supportsFunctions: true,
        },
        {
          id: 'gpt-3.5-turbo',
          name: 'GPT-3.5 Turbo',
          provider: 'openai',
          capabilities: ['text-generation', 'chat'],
          contextWindow: 16385,
          maxOutputTokens: 4096,
          costPer1kTokens: { input: 0.0005, output: 0.0015 },
          supportsStreaming: true,
          supportsFunctions: true,
        },
      ],
    });
  }

  /**
   * Override to calculate actual OpenAI costs
   */
  protected override calculateCost(usage: any, model: string): number {
    if (!usage) return 0;

    const modelConfig = this.config.models?.find(m => m.id === model);
    if (!modelConfig?.costPer1kTokens) return 0;

    const inputCost = (usage.prompt_tokens / 1000) * modelConfig.costPer1kTokens.input;
    const outputCost = (usage.completion_tokens / 1000) * modelConfig.costPer1kTokens.output;

    return inputCost + outputCost;
  }
}
