/**
 * FireworksAdapter - Fireworks AI API (OpenAI-compatible)
 *
 * Supports:
 * - Llama 3, Mixtral, DeepSeek
 * - Custom fine-tuning
 * - Fast inference with LoRA
 * - Function calling
 *
 * Just 20 lines thanks to BaseOpenAICompatibleAdapter!
 */

import { BaseOpenAICompatibleAdapter } from './BaseOpenAICompatibleAdapter';
import { getSecret } from '../../../../system/secrets/SecretManager';

export class FireworksAdapter extends BaseOpenAICompatibleAdapter {
  constructor(apiKey?: string) {
    super({
      providerId: 'fireworks',
      providerName: 'Fireworks AI',
      apiKey: apiKey || getSecret('FIREWORKS_API_KEY', 'FireworksAdapter') || '',
      baseUrl: 'https://api.fireworks.ai',
      defaultModel: 'accounts/fireworks/models/llama-v3p1-70b-instruct',
      timeout: 60000,
      supportedCapabilities: ['text-generation', 'chat', 'embeddings'],
      // Fireworks provides models via API, no need to hardcode
    });
  }
}
