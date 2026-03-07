/**
 * VisionInferenceProvider — Model selection + inference for vision descriptions.
 *
 * Responsibilities:
 * - Find available vision-capable models via AICapabilityRegistry
 * - Select best model (prefer local Candle, then preferred provider, then any)
 * - Build description prompts
 * - Execute multimodal inference via AIProviderDaemon
 * - Parse structured responses
 *
 * Separated from VisionDescriptionService so the inference layer is swappable:
 * - Today: LLaVA via TypeScript AIProviderDaemon
 * - Future: Native Candle LLaVA in Rust (Phase D)
 * - Fallback: Cloud vision APIs (Anthropic, OpenAI)
 */

import { AICapabilityRegistry } from '../../daemons/ai-provider-daemon/shared/AICapabilityRegistry';
import { AIProviderDaemon } from '../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import type { ChatMessage, ContentPart } from '../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import type { VisionDescription, DescribeOptions } from './VisionDescriptionService';

export class VisionInferenceProvider {
  /**
   * Check if any vision model is available for inference.
   */
  isAvailable(): boolean {
    const registry = AICapabilityRegistry.getInstance();
    return registry.findModelsWithCapability('image-input').length > 0;
  }

  /**
   * Get available vision models with their providers.
   */
  availableModels(): Array<{ modelId: string; provider: string }> {
    const registry = AICapabilityRegistry.getInstance();
    return registry.findModelsWithCapability('image-input').map(m => ({
      modelId: m.modelId,
      provider: m.providerId,
    }));
  }

  /**
   * Describe an image via multimodal inference.
   * Selects the best available model, builds prompt, calls AIProviderDaemon.
   */
  async describe(
    base64Data: string,
    mimeType: string,
    options: DescribeOptions = {}
  ): Promise<VisionDescription | null> {
    const startTime = Date.now();

    const selectedModel = this.selectModel(options);
    if (!selectedModel) return null;

    console.log(`[VisionInference] Selected: ${selectedModel.providerId}/${selectedModel.modelId}`);

    const prompt = options.prompt || this.buildPrompt(options);

    try {
      const imageContent: ContentPart = {
        type: 'image',
        image: { base64: base64Data, mimeType }
      };

      const textContent: ContentPart = {
        type: 'text',
        text: prompt
      };

      const message: ChatMessage = {
        role: 'user',
        content: [textContent, imageContent]
      };

      const response = await AIProviderDaemon.generateText({
        messages: [message],
        model: selectedModel.modelId,
        provider: selectedModel.providerId,
        maxTokens: options.maxLength ? Math.ceil(options.maxLength / 4) : 500,
        temperature: 0.3
      });

      if (response.finishReason === 'error' || !response.text) {
        console.error('[VisionInference] Generation failed:', response.error);
        return null;
      }

      const responseTime = Date.now() - startTime;
      const parsed = this.parseResponse(response.text, options);

      return {
        description: parsed.description || response.text,
        modelId: selectedModel.modelId,
        provider: selectedModel.providerId,
        timestamp: new Date().toISOString(),
        objects: parsed.objects,
        colors: parsed.colors,
        text: parsed.text,
        responseTimeMs: responseTime,
      };
    } catch (error) {
      console.error('[VisionInference] Error:', error);
      return null;
    }
  }

  /**
   * Select the best vision model based on options and availability.
   * Priority: preferredProvider > preferredModel > local Candle > first available.
   */
  private selectModel(options: DescribeOptions): { modelId: string; providerId: string } | null {
    const registry = AICapabilityRegistry.getInstance();
    const visionModels = registry.findModelsWithCapability('image-input');

    if (visionModels.length === 0) {
      console.warn('[VisionInference] No vision-capable models available');
      return null;
    }

    // Filter to configured providers (only providers with API keys or running services)
    const configuredProviders = new Set<string>();
    if (process.env.ANTHROPIC_API_KEY) configuredProviders.add('anthropic');
    if (process.env.OPENAI_API_KEY) configuredProviders.add('openai');
    if (process.env.GROQ_API_KEY) configuredProviders.add('groq');
    if (process.env.TOGETHER_API_KEY) configuredProviders.add('together');
    if (process.env.FIREWORKS_API_KEY) configuredProviders.add('fireworks');
    if (process.env.XAI_API_KEY) configuredProviders.add('xai');
    if (process.env.GOOGLE_API_KEY) configuredProviders.add('google');
    // Candle only if actually running (has vision models registered)
    const hasCandle = visionModels.some(m => m.providerId === 'candle');
    if (hasCandle) configuredProviders.add('candle');

    const available = visionModels.filter(m => configuredProviders.has(m.providerId));
    if (available.length === 0) {
      console.warn('[VisionInference] No vision models with configured providers');
      return null;
    }

    let selected = available[0];

    if (options.preferredModel) {
      const preferred = available.find(m => m.modelId === options.preferredModel);
      if (preferred) selected = preferred;
    }

    if (options.preferredProvider) {
      const preferred = available.find(m => m.providerId === options.preferredProvider);
      if (preferred) selected = preferred;
    }

    // Prefer local Candle when available (free, private) unless provider explicitly specified
    if (!options.preferredProvider && hasCandle) {
      const localModel = available.find(m => m.providerId === 'candle');
      if (localModel) selected = localModel;
    }

    return selected;
  }

  private buildPrompt(options: DescribeOptions): string {
    const parts: string[] = ['Describe this image concisely.'];
    if (options.detectObjects) parts.push('List the main objects you see.');
    if (options.detectColors) parts.push('Note the dominant colors.');
    if (options.detectText) parts.push('Read any text visible in the image.');
    if (options.maxLength) parts.push(`Keep the description under ${options.maxLength} characters.`);
    return parts.join(' ');
  }

  private parseResponse(
    text: string,
    _options: DescribeOptions
  ): { description: string; objects?: string[]; colors?: string[]; text?: string } {
    return { description: text.trim() };
  }
}
