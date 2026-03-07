/**
 * VisionDescriptionService — Facade for vision description generation + caching.
 *
 * "So the blind can see" — text-only models get descriptions of images.
 * Vision models get raw base64. Everyone gets the representation their capabilities support.
 *
 * Architecture (M1 split):
 * - VisionDescriptionCache: Content-addressed L1 cache + in-flight dedup
 * - VisionInferenceProvider: Model selection + multimodal inference
 * - VisionDescriptionService: Facade (this file) — unchanged public API
 *
 * Cache strategy:
 * - L1: In-memory Map (500 entries, access-based eviction after 30min idle)
 * - In-flight deduplication: 11 personas sharing one image = 1 inference call
 * - Future L2: Rust SQLite persistent cache (Phase B)
 */

import { VisionDescriptionCache } from './VisionDescriptionCache';
import { VisionInferenceProvider } from './VisionInferenceProvider';
import * as fs from 'fs';
import * as path from 'path';

/**
 * Description result with metadata
 */
export interface VisionDescription {
  description: string;
  modelId: string;
  provider: string;
  timestamp: string;
  objects?: string[];
  colors?: string[];
  text?: string;
  responseTimeMs: number;
}

/**
 * Options for description generation
 */
export interface DescribeOptions {
  preferredModel?: string;
  preferredProvider?: string;
  maxLength?: number;
  prompt?: string;
  detectObjects?: boolean;
  detectColors?: boolean;
  detectText?: boolean;
}

export class VisionDescriptionService {
  private static instance: VisionDescriptionService | null = null;

  private readonly _cache: VisionDescriptionCache;
  private readonly _inference: VisionInferenceProvider;

  constructor() {
    this._cache = new VisionDescriptionCache();
    this._inference = new VisionInferenceProvider();
  }

  static getInstance(): VisionDescriptionService {
    if (!this.instance) {
      this.instance = new VisionDescriptionService();
    }
    return this.instance;
  }

  /** Cache stats for diagnostics */
  get cacheStats(): { size: number; maxSize: number; inflightCount: number } {
    const stats = this._cache.stats;
    return { size: stats.l1Size, maxSize: stats.maxL1, inflightCount: stats.inflightCount };
  }

  /**
   * Check the status of a description for given base64 data.
   * Returns 'cached' if ready, 'inflight' if being processed, 'none' if unknown.
   * Used by MediaArtifactSource to decide timeout: cached=0s, inflight=90s, none=10s.
   */
  descriptionStatus(base64Data: string): 'cached' | 'inflight' | 'none' {
    const key = this._cache.contentKey(base64Data);
    return this._cache.status(key);
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
    const key = this._cache.contentKey(base64Data);

    // L1 cache hit — instant return
    const cached = this._cache.get(key);
    if (cached) {
      console.log(`[VisionDescription] Cache hit (key=${key.slice(0, 8)}), skipping inference`);
      return cached;
    }

    // In-flight deduplication — coalesce with existing request
    const inflight = this._cache.getInflight(key);
    if (inflight) {
      console.log(`[VisionDescription] Coalescing with in-flight request (key=${key.slice(0, 8)})`);
      return inflight;
    }

    // First caller — trigger inference
    const promise = this._inference.describe(base64Data, mimeType, options);
    this._cache.registerInflight(key, promise);

    try {
      const result = await promise;
      if (result) {
        this._cache.put(key, result);
      }
      return result;
    } finally {
      this._cache.clearInflight(key);
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
    return this._inference.isAvailable();
  }

  /**
   * Get available vision models
   */
  getAvailableModels(): Array<{ modelId: string; provider: string }> {
    return this._inference.availableModels();
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
