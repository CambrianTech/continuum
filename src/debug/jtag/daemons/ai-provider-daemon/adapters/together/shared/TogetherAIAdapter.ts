/**
 * TogetherAIAdapter - Together AI API (OpenAI-compatible)
 *
 * Supports:
 * - Llama 3.1 405B, 70B, 8B
 * - Mixtral, Qwen, DeepSeek
 * - Fine-tuning with LoRA
 * - Extremely fast inference
 *
 * Just 25 lines thanks to BaseOpenAICompatibleAdapter!
 * Together AI uses OpenAI's API format, so we inherit everything.
 */

import { BaseOpenAICompatibleAdapter } from '../../../shared/adapters/BaseOpenAICompatibleAdapter';
import { getSecret } from '../../../../../system/secrets/SecretManager';

export class TogetherAIAdapter extends BaseOpenAICompatibleAdapter {
  constructor(apiKey?: string) {
    super({
      providerId: 'together',
      providerName: 'Together AI',
      apiKey: apiKey || getSecret('TOGETHER_API_KEY', 'TogetherAIAdapter') || '',
      baseUrl: 'https://api.together.xyz',
      defaultModel: 'meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo',
      timeout: 60000,
      supportedCapabilities: [
        'text-generation',
        'chat',
        'embeddings',
      ],
      models: [
        {
          id: 'meta-llama/Meta-Llama-3.1-405B-Instruct-Turbo',
          name: 'Llama 3.1 405B Instruct Turbo',
          provider: 'together',
          capabilities: ['text-generation', 'chat'],
          contextWindow: 128000,
          maxOutputTokens: 4096,
          costPer1kTokens: { input: 0.005, output: 0.015 },
          supportsStreaming: true,
          supportsFunctions: false,
        },
        {
          id: 'meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo',
          name: 'Llama 3.1 70B Instruct Turbo',
          provider: 'together',
          capabilities: ['text-generation', 'chat'],
          contextWindow: 128000,
          maxOutputTokens: 4096,
          costPer1kTokens: { input: 0.0009, output: 0.0009 },
          supportsStreaming: true,
          supportsFunctions: false,
        },
        {
          id: 'meta-llama/Meta-Llama-3.1-8B-Instruct-Turbo',
          name: 'Llama 3.1 8B Instruct Turbo',
          provider: 'together',
          capabilities: ['text-generation', 'chat'],
          contextWindow: 128000,
          maxOutputTokens: 4096,
          costPer1kTokens: { input: 0.0002, output: 0.0002 },
          supportsStreaming: true,
          supportsFunctions: false,
        },
      ],
    });
  }
}
