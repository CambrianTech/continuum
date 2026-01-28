/**
 * GoogleAdapter - Google Gemini via OpenAI-compatible API
 *
 * Supports:
 * - Gemini 2.5 Flash (latest, free tier available)
 * - Gemini 2.0 Flash
 * - Gemini 1.5 Flash (fast, cheap)
 * - Gemini 1.5 Pro (powerful, 2M context)
 *
 * Just 30 lines of code thanks to BaseOpenAICompatibleAdapter!
 *
 * Google provides an OpenAI-compatible endpoint at:
 * https://generativelanguage.googleapis.com/v1beta/openai
 *
 * Note: For audio-native (real-time voice), use GeminiLiveAdapter instead.
 * This adapter is for text-based inference only.
 */

import { BaseOpenAICompatibleAdapter } from '../../../shared/adapters/BaseOpenAICompatibleAdapter';
import type { ModelInfo } from '../../../shared/AIProviderTypesV2';
import { GoogleBaseConfig } from './GoogleBaseConfig';

export class GoogleAdapter extends BaseOpenAICompatibleAdapter {
  private readonly sharedConfig: GoogleBaseConfig;

  constructor(apiKey?: string) {
    // Create shared config (used by inference + audio-native adapters)
    const sharedConfig = new GoogleBaseConfig(apiKey);

    super({
      providerId: sharedConfig.providerId,
      providerName: sharedConfig.providerName,
      apiKey: sharedConfig.apiKey,
      baseUrl: sharedConfig.baseUrl,
      defaultModel: sharedConfig.getDefaultModel(),
      timeout: 120000, // 2 minutes for large context requests
      supportedCapabilities: [
        'text-generation',
        'chat',
        'multimodal',
        'image-analysis',
      ],
      // Only include text models, not audio-native
      models: sharedConfig.getTextModels(),
    });

    this.sharedConfig = sharedConfig;
  }

  getSharedConfig(): GoogleBaseConfig {
    return this.sharedConfig;
  }

  async getAvailableModels(): Promise<ModelInfo[]> {
    return this.config.models ?? [];
  }
}
