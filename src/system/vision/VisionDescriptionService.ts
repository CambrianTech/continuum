/**
 * VisionDescriptionService - Auto-describe visual content for non-vision AIs
 * ============================================================================
 *
 * Provides automatic vision descriptions for visual content (canvas, screenshots,
 * images). Enables non-vision-capable AIs to understand visual context through
 * text descriptions stored as metadata.
 *
 * Pattern: "So the blind can see"
 * - Vision-capable AI describes the image
 * - Description stored as entity metadata
 * - Non-vision AIs access the description
 *
 * Usage:
 *   const description = await VisionDescriptionService.describe(imageBase64);
 *   entity.metadata.description = description;
 */

import { AICapabilityRegistry } from '../../daemons/ai-provider-daemon/shared/AICapabilityRegistry';
import { VisionCapabilityService } from '../../daemons/ai-provider-daemon/shared/VisionCapabilityService';
import { AIProviderDaemon } from '../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import type { ChatMessage, ContentPart } from '../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import { createHash } from 'crypto';
import * as fs from 'fs';
import * as path from 'path';

/**
 * Description result with metadata
 */
export interface VisionDescription {
  /** The description text */
  description: string;

  /** Model that generated the description */
  modelId: string;

  /** Provider of the model */
  provider: string;

  /** When the description was generated */
  timestamp: string;

  /** Detected objects (if available) */
  objects?: string[];

  /** Dominant colors (if available) */
  colors?: string[];

  /** OCR text (if available) */
  text?: string;

  /** Response time in ms */
  responseTimeMs: number;
}

/**
 * Options for description generation
 */
export interface DescribeOptions {
  /** Preferred model to use (optional) */
  preferredModel?: string;

  /** Preferred provider (optional) */
  preferredProvider?: string;

  /** Maximum description length in characters */
  maxLength?: number;

  /** Custom prompt (default: general description) */
  prompt?: string;

  /** Include object detection */
  detectObjects?: boolean;

  /** Include color analysis */
  detectColors?: boolean;

  /** Include OCR */
  detectText?: boolean;
}

/**
 * Service for generating vision descriptions
 */
/**
 * Content-addressed description cache entry.
 * Key = hash of image content. One LLaVA call serves all consumers.
 */
interface CachedDescription {
  description: VisionDescription;
  cachedAt: number;
}

/** Cache TTL: 5 minutes. Images don't change — but descriptions may improve with better models. */
const DESCRIPTION_CACHE_TTL_MS = 5 * 60 * 1000;

/** Max cache entries. Each is ~1KB (description text + metadata). */
const MAX_CACHE_ENTRIES = 500;

export class VisionDescriptionService {
  private static instance: VisionDescriptionService | null = null;

  /** Content-addressed cache: hash(base64) → description. Process once, serve many. */
  private readonly _cache = new Map<string, CachedDescription>();

  /** In-flight deduplication: hash → promise. Prevents N concurrent calls for the same image. */
  private readonly _inflight = new Map<string, Promise<VisionDescription | null>>();

  static getInstance(): VisionDescriptionService {
    if (!this.instance) {
      this.instance = new VisionDescriptionService();
    }
    return this.instance;
  }

  /** Cache stats for diagnostics */
  get cacheStats(): { size: number; maxSize: number; ttlMs: number } {
    return { size: this._cache.size, maxSize: MAX_CACHE_ENTRIES, ttlMs: DESCRIPTION_CACHE_TTL_MS };
  }

  /**
   * Check the status of a description for given base64 data.
   * Returns 'cached' if ready, 'inflight' if being processed, 'none' if unknown.
   * Used by MediaArtifactSource to decide timeout: cached=0s, inflight=90s, none=10s.
   */
  descriptionStatus(base64Data: string): 'cached' | 'inflight' | 'none' {
    const key = this.contentKey(base64Data);
    const cached = this._cache.get(key);
    if (cached && (Date.now() - cached.cachedAt) < DESCRIPTION_CACHE_TTL_MS) {
      return 'cached';
    }
    if (this._inflight.has(key)) {
      return 'inflight';
    }
    return 'none';
  }

  /**
   * Content-address key: SHA-256 of first 4KB + total length.
   * Hashing the full base64 of a 5MB image would be wasteful — first 4KB + length
   * is sufficient to distinguish images while keeping key generation <1ms.
   */
  private contentKey(base64Data: string): string {
    const sample = base64Data.slice(0, 4096);
    return createHash('sha256').update(`${sample}:${base64Data.length}`).digest('hex').slice(0, 16);
  }

  /**
   * Describe an image from base64 data.
   *
   * Content-addressed cache + in-flight deduplication:
   * - First caller triggers inference, all concurrent callers await the same promise
   * - Result cached by content hash — subsequent calls return instantly
   * - 11 personas sharing one image = 1 LLaVA call, not 11
   */
  async describeBase64(
    base64Data: string,
    mimeType: string = 'image/png',
    options: DescribeOptions = {}
  ): Promise<VisionDescription | null> {
    const key = this.contentKey(base64Data);

    // Check cache first — instant return for previously described images
    const cached = this._cache.get(key);
    if (cached && (Date.now() - cached.cachedAt) < DESCRIPTION_CACHE_TTL_MS) {
      console.log(`[VisionDescription] Cache hit (key=${key.slice(0, 8)}), skipping inference`);
      return cached.description;
    }

    // In-flight deduplication — if another caller is already describing this image, await their result
    const inflight = this._inflight.get(key);
    if (inflight) {
      console.log(`[VisionDescription] Coalescing with in-flight request (key=${key.slice(0, 8)})`);
      return inflight;
    }

    // First caller — trigger inference and register the in-flight promise
    const promise = this._describeBase64Uncached(base64Data, mimeType, options, key);
    this._inflight.set(key, promise);

    try {
      const result = await promise;

      // Cache the result
      if (result) {
        // LRU eviction: drop oldest entry if at capacity
        if (this._cache.size >= MAX_CACHE_ENTRIES) {
          const oldestKey = this._cache.keys().next().value;
          if (oldestKey) this._cache.delete(oldestKey);
        }
        this._cache.set(key, { description: result, cachedAt: Date.now() });
      }

      return result;
    } finally {
      this._inflight.delete(key);
    }
  }

  /**
   * Internal: actual inference call (no caching). Called only by describeBase64.
   */
  private async _describeBase64Uncached(
    base64Data: string,
    mimeType: string,
    options: DescribeOptions,
    cacheKey: string
  ): Promise<VisionDescription | null> {
    const startTime = Date.now();

    // Find a vision-capable model
    const registry = AICapabilityRegistry.getInstance();
    const visionModels = registry.findModelsWithCapability('image-input');

    if (visionModels.length === 0) {
      console.warn('[VisionDescription] No vision-capable models available');
      return null;
    }

    // Filter to only configured providers (check environment for API keys)
    const configuredProviders = new Set<string>();
    if (process.env.ANTHROPIC_API_KEY) configuredProviders.add('anthropic');
    if (process.env.OPENAI_API_KEY) configuredProviders.add('openai');
    if (process.env.GROQ_API_KEY) configuredProviders.add('groq');
    if (process.env.TOGETHER_API_KEY) configuredProviders.add('together');
    // Candle is always available (built-in local inference)
    configuredProviders.add('candle');

    // Filter vision models to only those with configured providers
    const availableVisionModels = visionModels.filter(m => configuredProviders.has(m.providerId));

    if (availableVisionModels.length === 0) {
      console.warn('[VisionDescription] No vision-capable models with configured providers');
      console.warn('[VisionDescription] Configured providers:', Array.from(configuredProviders));
      console.warn('[VisionDescription] Vision models found:', visionModels.map(m => `${m.providerId}/${m.modelId}`));
      return null;
    }

    // Select model (prefer specified, then local, then any)
    let selectedModel = availableVisionModels[0];

    if (options.preferredModel) {
      const preferred = availableVisionModels.find(m => m.modelId === options.preferredModel);
      if (preferred) selectedModel = preferred;
    }

    if (options.preferredProvider) {
      const preferred = availableVisionModels.find(m => m.providerId === options.preferredProvider);
      if (preferred) selectedModel = preferred;
    }

    // Prefer local Candle models (free, private) if available
    const localModel = availableVisionModels.find(m => m.providerId === 'candle');
    if (localModel && !options.preferredProvider) {
      selectedModel = localModel;
    }

    console.log(`[VisionDescription] Selected model: ${selectedModel.providerId}/${selectedModel.modelId} (key=${cacheKey.slice(0, 8)})`);

    // Build prompt
    const prompt = options.prompt || this.buildDescriptionPrompt(options);

    try {
      // Build multimodal message with image
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

      // Generate description via AIProviderDaemon (supports multimodal)
      const response = await AIProviderDaemon.generateText({
        messages: [message],
        model: selectedModel.modelId,
        provider: selectedModel.providerId,
        maxTokens: options.maxLength ? Math.ceil(options.maxLength / 4) : 500,
        temperature: 0.3  // More deterministic for descriptions
      });

      if (response.finishReason === 'error' || !response.text) {
        console.error('[VisionDescription] Generation failed:', response.error);
        return null;
      }

      const text = response.text;

      const responseTime = Date.now() - startTime;

      // Parse response for structured data
      const parsedResponse = this.parseResponse(text, options);

      return {
        description: parsedResponse.description || text,
        modelId: selectedModel.modelId,
        provider: selectedModel.providerId,
        timestamp: new Date().toISOString(),
        objects: parsedResponse.objects,
        colors: parsedResponse.colors,
        text: parsedResponse.text,
        responseTimeMs: responseTime,
      };
    } catch (error) {
      console.error('[VisionDescription] Error:', error);
      return null;
    }
  }

  /**
   * Describe an image from file path
   */
  async describeFile(
    filePath: string,
    options: DescribeOptions = {}
  ): Promise<VisionDescription | null> {
    try {
      const absolutePath = path.resolve(filePath);
      const buffer = fs.readFileSync(absolutePath);
      const base64 = buffer.toString('base64');

      // Determine MIME type from extension
      const ext = path.extname(filePath).toLowerCase();
      const mimeTypes: Record<string, string> = {
        '.png': 'image/png',
        '.jpg': 'image/jpeg',
        '.jpeg': 'image/jpeg',
        '.gif': 'image/gif',
        '.webp': 'image/webp',
      };
      const mimeType = mimeTypes[ext] || 'image/png';

      return this.describeBase64(base64, mimeType, options);
    } catch (error) {
      console.error('[VisionDescription] Failed to read file:', error);
      return null;
    }
  }

  /**
   * Check if vision description is available
   */
  isAvailable(): boolean {
    const registry = AICapabilityRegistry.getInstance();
    const visionModels = registry.findModelsWithCapability('image-input');
    return visionModels.length > 0;
  }

  /**
   * Get available vision models
   */
  getAvailableModels(): Array<{ modelId: string; provider: string }> {
    const registry = AICapabilityRegistry.getInstance();
    return registry.findModelsWithCapability('image-input').map(m => ({
      modelId: m.modelId,
      provider: m.providerId,
    }));
  }

  /**
   * Build description prompt based on options
   */
  private buildDescriptionPrompt(options: DescribeOptions): string {
    const parts: string[] = [];

    parts.push('Describe this image concisely.');

    if (options.detectObjects) {
      parts.push('List the main objects you see.');
    }

    if (options.detectColors) {
      parts.push('Note the dominant colors.');
    }

    if (options.detectText) {
      parts.push('Read any text visible in the image.');
    }

    if (options.maxLength) {
      parts.push(`Keep the description under ${options.maxLength} characters.`);
    }

    return parts.join(' ');
  }

  /**
   * Parse response to extract structured data
   */
  private parseResponse(
    text: string,
    options: DescribeOptions
  ): {
    description: string;
    objects?: string[];
    colors?: string[];
    text?: string;
  } {
    // For now, just return the text as description
    // Future: Parse structured data from response
    return {
      description: text.trim(),
    };
  }
}

/**
 * Convenience function for quick descriptions
 */
export async function describeImage(
  imageData: string | { base64: string; mimeType?: string } | { filePath: string },
  options?: DescribeOptions
): Promise<VisionDescription | null> {
  const service = VisionDescriptionService.getInstance();

  if (typeof imageData === 'string') {
    // Could be base64 or file path
    if (imageData.startsWith('/') || imageData.includes('.')) {
      return service.describeFile(imageData, options);
    }
    return service.describeBase64(imageData, 'image/png', options);
  }

  if ('filePath' in imageData) {
    return service.describeFile(imageData.filePath, options);
  }

  return service.describeBase64(
    imageData.base64,
    imageData.mimeType || 'image/png',
    options
  );
}
