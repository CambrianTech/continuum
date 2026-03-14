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
import type { ProviderCapabilities } from '../../../shared/AICapabilityRegistry';
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

  protected getCapabilityRegistration(): ProviderCapabilities {
    return {
      providerId: 'google',
      providerName: 'Google',
      defaultCapabilities: [
        'text-input', 'text-output', 'image-input', 'function-calling',
        'streaming', 'multimodal',
      ],
      models: [
        {
          modelId: 'gemini-2.5-flash-preview-05-20',
          displayName: 'Gemini 2.5 Flash',
          capabilities: ['reasoning', 'coding', 'context-window-huge', 'math',
            'prose', 'planning', 'review', 'research', 'instruction-following', 'tool-use'],
          contextWindow: 1048576,
          costTier: 'low',
          latencyTier: 'fast',
        },
        {
          modelId: 'gemini-2.0-flash',
          displayName: 'Gemini 2.0 Flash',
          capabilities: ['reasoning', 'coding', 'context-window-huge',
            'prose', 'planning', 'instruction-following', 'tool-use'],
          contextWindow: 1048576,
          costTier: 'low',
          latencyTier: 'fast',
        },
        {
          modelId: 'gemini-1.5-flash',
          displayName: 'Gemini 1.5 Flash',
          capabilities: ['coding', 'context-window-huge', 'instruction-following'],
          contextWindow: 1048576,
          costTier: 'free',
          latencyTier: 'fast',
        },
        {
          modelId: 'gemini-1.5-pro',
          displayName: 'Gemini 1.5 Pro',
          capabilities: ['reasoning', 'coding', 'context-window-huge',
            'prose', 'planning', 'review', 'research', 'tool-use'],
          contextWindow: 2097152,
          costTier: 'medium',
          latencyTier: 'medium',
        },
      ],
    };
  }
}
