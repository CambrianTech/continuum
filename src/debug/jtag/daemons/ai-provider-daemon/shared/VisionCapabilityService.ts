/**
 * VisionCapabilityService - Centralized Vision Model Registry
 * =============================================================
 *
 * Single source of truth for which models support vision/multimodal input.
 *
 * Architecture:
 * - Adapters register their vision-capable models on initialization
 * - Runtime queries check if a specific model supports vision
 * - Supports pattern-based matching for model families (e.g., "llava*", "gpt-4*-vision")
 *
 * Why centralized?
 * - Eliminates scattered supportsVision checks in each adapter
 * - Single place to update when new vision models are released
 * - Enables cross-adapter vision capability queries
 * - Supports eventual Rust worker architecture (single registry)
 */

export interface VisionModelEntry {
  modelId: string;           // Exact model ID or pattern with wildcards
  provider: string;          // Provider ID (candle, anthropic, openai, etc.)
  isPattern: boolean;        // True if modelId contains wildcards
  capabilities: VisionCapability[];
  maxImageSize?: number;     // Max image dimension in pixels
  maxImagesPerRequest?: number;
  supportedFormats?: string[]; // e.g., ['png', 'jpeg', 'gif', 'webp']
}

export type VisionCapability =
  | 'image-input'        // Can accept images in prompts
  | 'image-generation'   // Can generate images
  | 'image-analysis'     // Can analyze/describe images
  | 'video-input'        // Can accept video
  | 'video-generation';  // Can generate video

/**
 * Vision Capability Service - Singleton
 *
 * Usage:
 *   const vision = VisionCapabilityService.getInstance();
 *
 *   // Check if model supports vision
 *   if (vision.supportsVision('candle', 'llava:latest')) {
 *     // Use vision-enabled code path
 *   }
 *
 *   // Get all vision models for a provider
 *   const localVisionModels = vision.getVisionModels('candle');
 */
export class VisionCapabilityService {
  private static instance: VisionCapabilityService | null = null;

  // Registry: provider -> model entries
  private registry: Map<string, VisionModelEntry[]> = new Map();

  // Cache for pattern matching results
  private matchCache: Map<string, boolean> = new Map();

  private constructor() {
    // Initialize with known vision models
    this.initializeBuiltInModels();
  }

  static getInstance(): VisionCapabilityService {
    if (!VisionCapabilityService.instance) {
      VisionCapabilityService.instance = new VisionCapabilityService();
    }
    return VisionCapabilityService.instance;
  }

  /**
   * Register a vision-capable model
   * Called by adapters during initialization
   */
  registerVisionModel(entry: VisionModelEntry): void {
    const existing = this.registry.get(entry.provider) || [];

    // Avoid duplicates
    const isDuplicate = existing.some(e =>
      e.modelId === entry.modelId && e.provider === entry.provider
    );

    if (!isDuplicate) {
      existing.push(entry);
      this.registry.set(entry.provider, existing);

      // Clear cache for this provider
      this.clearProviderCache(entry.provider);
    }
  }

  /**
   * Bulk register vision models for a provider
   */
  registerVisionModels(provider: string, models: Omit<VisionModelEntry, 'provider'>[]): void {
    for (const model of models) {
      this.registerVisionModel({ ...model, provider });
    }
  }

  /**
   * Check if a specific model supports vision input
   */
  supportsVision(provider: string, modelId: string): boolean {
    const cacheKey = `${provider}:${modelId}`;

    // Check cache first
    if (this.matchCache.has(cacheKey)) {
      return this.matchCache.get(cacheKey)!;
    }

    const entries = this.registry.get(provider) || [];

    for (const entry of entries) {
      if (this.matchesModel(entry, modelId)) {
        // Must have image-input or image-analysis capability
        const hasVision = entry.capabilities.includes('image-input') ||
                         entry.capabilities.includes('image-analysis');
        this.matchCache.set(cacheKey, hasVision);
        return hasVision;
      }
    }

    // No match found
    this.matchCache.set(cacheKey, false);
    return false;
  }

  /**
   * Get vision capabilities for a specific model
   * Returns null if model is not vision-capable
   */
  getCapabilities(provider: string, modelId: string): VisionModelEntry | null {
    const entries = this.registry.get(provider) || [];

    for (const entry of entries) {
      if (this.matchesModel(entry, modelId)) {
        return entry;
      }
    }

    return null;
  }

  /**
   * Get all vision-capable models for a provider
   */
  getVisionModels(provider: string): VisionModelEntry[] {
    return this.registry.get(provider) || [];
  }

  /**
   * Get all registered providers
   */
  getProviders(): string[] {
    return Array.from(this.registry.keys());
  }

  /**
   * Check if model ID matches entry (supports wildcards)
   */
  private matchesModel(entry: VisionModelEntry, modelId: string): boolean {
    if (!entry.isPattern) {
      // Exact match
      return entry.modelId === modelId ||
             modelId.startsWith(entry.modelId + ':'); // Handle tags like "llava:latest"
    }

    // Pattern matching with wildcards
    const pattern = entry.modelId
      .replace(/\*/g, '.*')  // * -> .*
      .replace(/\?/g, '.');  // ? -> .

    const regex = new RegExp(`^${pattern}$`, 'i');
    return regex.test(modelId);
  }

  /**
   * Clear cache for a provider (called when models are registered)
   */
  private clearProviderCache(provider: string): void {
    const keysToDelete: string[] = [];
    for (const key of this.matchCache.keys()) {
      if (key.startsWith(`${provider}:`)) {
        keysToDelete.push(key);
      }
    }
    for (const key of keysToDelete) {
      this.matchCache.delete(key);
    }
  }

  /**
   * Initialize built-in vision models
   * These are the known vision models across all providers
   */
  private initializeBuiltInModels(): void {
    // ================================
    // Local Vision Models (Candle)
    // ================================
    this.registerVisionModels('candle', [
      {
        modelId: 'llava*',
        isPattern: true,
        capabilities: ['image-input', 'image-analysis'],
        maxImagesPerRequest: 1,
        supportedFormats: ['png', 'jpeg', 'gif', 'webp'],
      },
      {
        modelId: 'llama3.2-vision*',
        isPattern: true,
        capabilities: ['image-input', 'image-analysis'],
        maxImagesPerRequest: 1,
        supportedFormats: ['png', 'jpeg', 'gif', 'webp'],
      },
      {
        modelId: 'bakllava*',
        isPattern: true,
        capabilities: ['image-input', 'image-analysis'],
        maxImagesPerRequest: 1,
        supportedFormats: ['png', 'jpeg', 'gif', 'webp'],
      },
      {
        modelId: 'moondream*',
        isPattern: true,
        capabilities: ['image-input', 'image-analysis'],
        maxImagesPerRequest: 1,
        supportedFormats: ['png', 'jpeg', 'gif', 'webp'],
      },
      {
        modelId: 'minicpm-v*',
        isPattern: true,
        capabilities: ['image-input', 'image-analysis'],
        maxImagesPerRequest: 1,
        supportedFormats: ['png', 'jpeg', 'gif', 'webp'],
      },
    ]);

    // ================================
    // Anthropic Vision Models
    // ================================
    this.registerVisionModels('anthropic', [
      {
        modelId: 'claude-3*',
        isPattern: true,
        capabilities: ['image-input', 'image-analysis'],
        maxImagesPerRequest: 20,
        maxImageSize: 8000,
        supportedFormats: ['png', 'jpeg', 'gif', 'webp'],
      },
      {
        modelId: 'claude-sonnet-4*',
        isPattern: true,
        capabilities: ['image-input', 'image-analysis'],
        maxImagesPerRequest: 20,
        maxImageSize: 8000,
        supportedFormats: ['png', 'jpeg', 'gif', 'webp'],
      },
      {
        modelId: 'claude-opus-4*',
        isPattern: true,
        capabilities: ['image-input', 'image-analysis'],
        maxImagesPerRequest: 20,
        maxImageSize: 8000,
        supportedFormats: ['png', 'jpeg', 'gif', 'webp'],
      },
    ]);

    // ================================
    // OpenAI Vision Models
    // ================================
    this.registerVisionModels('openai', [
      {
        modelId: 'gpt-4-vision*',
        isPattern: true,
        capabilities: ['image-input', 'image-analysis'],
        maxImagesPerRequest: 10,
        supportedFormats: ['png', 'jpeg', 'gif', 'webp'],
      },
      {
        modelId: 'gpt-4o*',
        isPattern: true,
        capabilities: ['image-input', 'image-analysis'],
        maxImagesPerRequest: 10,
        supportedFormats: ['png', 'jpeg', 'gif', 'webp'],
      },
      {
        modelId: 'gpt-4-turbo*',
        isPattern: true,
        capabilities: ['image-input', 'image-analysis'],
        maxImagesPerRequest: 10,
        supportedFormats: ['png', 'jpeg', 'gif', 'webp'],
      },
    ]);

    // ================================
    // Together AI Vision Models
    // ================================
    this.registerVisionModels('together', [
      {
        modelId: 'meta-llama/Llama-3.2-*Vision*',
        isPattern: true,
        capabilities: ['image-input', 'image-analysis'],
        maxImagesPerRequest: 1,
        supportedFormats: ['png', 'jpeg', 'gif', 'webp'],
      },
      {
        modelId: '*llava*',
        isPattern: true,
        capabilities: ['image-input', 'image-analysis'],
        maxImagesPerRequest: 1,
        supportedFormats: ['png', 'jpeg', 'gif', 'webp'],
      },
    ]);

    // ================================
    // XAI (Grok) Vision Models
    // ================================
    this.registerVisionModels('xai', [
      {
        modelId: 'grok-vision*',
        isPattern: true,
        capabilities: ['image-input', 'image-analysis'],
        maxImagesPerRequest: 5,
        supportedFormats: ['png', 'jpeg', 'gif', 'webp'],
      },
      {
        modelId: 'grok-2-vision*',
        isPattern: true,
        capabilities: ['image-input', 'image-analysis'],
        maxImagesPerRequest: 5,
        supportedFormats: ['png', 'jpeg', 'gif', 'webp'],
      },
    ]);

    // ================================
    // Fireworks Vision Models
    // ================================
    this.registerVisionModels('fireworks', [
      {
        modelId: '*llava*',
        isPattern: true,
        capabilities: ['image-input', 'image-analysis'],
        maxImagesPerRequest: 1,
        supportedFormats: ['png', 'jpeg', 'gif', 'webp'],
      },
    ]);
  }

  /**
   * Debug: Print all registered models
   */
  debugPrint(): void {
    console.log('=== VisionCapabilityService Registry ===');
    for (const [provider, entries] of this.registry) {
      console.log(`\n${provider}:`);
      for (const entry of entries) {
        console.log(`  - ${entry.modelId} ${entry.isPattern ? '(pattern)' : ''}`);
        console.log(`    Capabilities: ${entry.capabilities.join(', ')}`);
      }
    }
  }
}

// Export singleton accessor
export const visionService = VisionCapabilityService.getInstance;
