/**
 * FireworksAdapter - Fireworks AI API (OpenAI-compatible)
 *
 * Fireworks AI provides fast inference with diverse open models:
 * - DeepSeek V3.1 (reasoning model)
 * - Llama, Mixtral, Qwen models
 * - Function calling & structured outputs
 * - Vision models (Qwen2.5-VL)
 * - Custom fine-tuning with LoRA
 *
 * API: OpenAI-compatible format
 * Base URL: https://api.fireworks.ai/inference/v1
 * Docs: https://docs.fireworks.ai/
 */

import { BaseOpenAICompatibleAdapter } from '../../../shared/adapters/BaseOpenAICompatibleAdapter';
import { getSecret } from '../../../../../system/secrets/SecretManager';

export class FireworksAdapter extends BaseOpenAICompatibleAdapter {
  constructor(apiKey?: string) {
    super({
      providerId: 'fireworks',
      providerName: 'Fireworks AI',
      apiKey: apiKey || getSecret('FIREWORKS_API_KEY', 'FireworksAdapter') || '',
      baseUrl: 'https://api.fireworks.ai/inference',
      defaultModel: 'accounts/fireworks/models/deepseek-v3p1',
      timeout: 60000,
      supportedCapabilities: ['text-generation', 'chat', 'embeddings'],
      // Fireworks provides models via API, no need to hardcode
    });
  }
}
