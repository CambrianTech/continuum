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

    // L1 cache hit — instant return (per-process, lost on restart)
    const cached = this._cache.get(key);
    if (cached) {
      console.log(`[VisionDescription] Cache hit (key=${key.slice(0, 8)}), skipping inference`);
      return cached;
    }

    // L1.5 cache (Rust HashMap) — sub-ms IPC, lost on Rust restart
    const rustCached = await this._cache.getFromRust(key);
    if (rustCached) {
      console.log(`[VisionDescription] Rust L1.5 hit (key=${key.slice(0, 8)}), skipping inference`);
      return rustCached;
    }

    // L2 sidecar JSON on disk — survives every restart. Joel's
    // 2026-04-21 directive: "we run yolo or whatever ONCE per data
    // and keep track of it". Content-addressed sidecar means every
    // unique image gets exactly one vision-inference per machine
    // forever, regardless of how many TS/Rust process bounces happen.
    // Cheap (single file stat + JSON.parse) so safe to check on the
    // hot path.
    const blobHash = `sha256:${key}`;  // contentKey is already hex sha256 of binary
    try {
      const { MediaBlobService } = await import('../storage/MediaBlobService');
      const sidecar = await MediaBlobService.readSidecar(blobHash);
      if (sidecar?.description) {
        const fromDisk: VisionDescription = {
          description: sidecar.description,
          modelId: sidecar.generatedBy ?? 'sidecar',
          provider: 'sidecar',
          timestamp: new Date(sidecar.generatedAtMs ?? Date.now()).toISOString(),
          responseTimeMs: 0,
        };
        // Promote to L1 + L1.5 so subsequent calls in this process
        // don't even hit the disk.
        this._cache.put(key, fromDisk);
        console.log(`[VisionDescription] Sidecar L2 hit (key=${key.slice(0, 8)}), skipping inference`);
        return fromDisk;
      }
    } catch {
      // Sidecar lookup is best-effort. Fall through to inference.
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
        // Persist to L2 sidecar so the next process restart finds it
        // without re-running inference. Fire-and-forget — sidecar write
        // failure shouldn't fail the request, but log for diagnostics.
        try {
          const { MediaBlobService } = await import('../storage/MediaBlobService');
          await MediaBlobService.writeSidecar(blobHash, {
            description: result.description,
            mimeType,
            generatedBy: result.modelId,
            generatedAtMs: Date.now(),
          });
        } catch (err) {
          console.warn(`[VisionDescription] sidecar write failed for ${blobHash.slice(0, 16)}:`, err);
        }
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
   * Best-effort "is a vision model registered?" check, kept synchronous
   * for the existing fast-fail call sites (MediaPrewarmServerCommand,
   * LiveRoomSnapshotService, MediaArtifactSource — all `if (!isAvailable())
   * skip-this-work`).
   *
   * Post-#1276 the source-of-truth lives in the Rust model registry;
   * the only honest synchronous answer is "true (probably) — call
   * `describe()` and it will return `null` if no vision model is
   * actually loadable." All three current callers handle a `null`
   * result gracefully (skip / return-empty), so this preserves the
   * pre-existing behavior without a sync IPC roundtrip on every guard.
   *
   * Future card: replace this with an async, registry-backed check via
   * the upcoming `ai/providers/list` IPC + `capability=vision` filter,
   * and migrate all three call sites to await it.
   */
  isAvailable(): boolean {
    return true;
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
