/**
 * RustCoreIPC Vision Cache Module — L1.5 content-addressed cache in Rust
 *
 * The Rust VisionModule provides an in-process HashMap cache (L1.5) between
 * the TS in-memory Map (L1) and ORM persistence (L2).
 *
 * Commands: vision/description-get, vision/description-put, vision/description-status,
 *           vision/cache-stats, vision/cache-warm, vision/cache-evict
 */

import type { RustCoreIPCClientBase } from './base';

// ============================================================================
// Types
// ============================================================================

export interface VisionCacheEntry {
	description: string;
	model: string;
	provider: string;
	processingTimeMs: number;
	confidence: number;
}

export interface VisionCacheGetResult {
	found: boolean;
	description?: string;
	model?: string;
	provider?: string;
	processingTimeMs?: number;
	confidence?: number;
}

export interface VisionCacheStats {
	entries: number;
	maxEntries: number;
	hits: number;
	misses: number;
	hitRate: number;
	evictions: number;
}

export interface VisionCacheWarmEntry {
	contentKey: string;
	description: string;
	model?: string;
	provider?: string;
	processingTimeMs?: number;
	confidence?: number;
	cachedAt?: number;
}

// ============================================================================
// Mixin
// ============================================================================

export interface VisionCacheMixin {
	visionCacheGet(contentKey: string): Promise<VisionCacheGetResult>;
	visionCachePut(contentKey: string, entry: VisionCacheEntry): Promise<void>;
	visionCacheStatus(contentKey: string): Promise<'cached' | 'none'>;
	visionCacheStats(): Promise<VisionCacheStats>;
	visionCacheWarm(entries: VisionCacheWarmEntry[]): Promise<{ warmed: number; total: number }>;
	visionCacheEvict(idleMs?: number): Promise<{ evicted: number; remaining: number }>;
}

export function VisionCacheMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements VisionCacheMixin {
		async visionCacheGet(contentKey: string): Promise<VisionCacheGetResult> {
			const response = await this.request({ command: 'vision/description-get', content_key: contentKey });
			if (!response.success) throw new Error(response.error || 'vision/description-get failed');
			const r = response.result as Record<string, unknown>;
			return {
				found: r.found as boolean,
				description: r.description as string | undefined,
				model: r.model as string | undefined,
				provider: r.provider as string | undefined,
				processingTimeMs: r.processing_time_ms as number | undefined,
				confidence: r.confidence as number | undefined,
			};
		}

		async visionCachePut(contentKey: string, entry: VisionCacheEntry): Promise<void> {
			const response = await this.request({
				command: 'vision/description-put',
				content_key: contentKey,
				description: entry.description,
				model: entry.model,
				provider: entry.provider,
				processing_time_ms: entry.processingTimeMs,
				confidence: entry.confidence,
			});
			if (!response.success) throw new Error(response.error || 'vision/description-put failed');
		}

		async visionCacheStatus(contentKey: string): Promise<'cached' | 'none'> {
			const response = await this.request({ command: 'vision/description-status', content_key: contentKey });
			if (!response.success) throw new Error(response.error || 'vision/description-status failed');
			return (response.result as { status: string }).status as 'cached' | 'none';
		}

		async visionCacheStats(): Promise<VisionCacheStats> {
			const response = await this.request({ command: 'vision/cache-stats' });
			if (!response.success) throw new Error(response.error || 'vision/cache-stats failed');
			const r = response.result as Record<string, number>;
			return {
				entries: r.entries,
				maxEntries: r.max_entries,
				hits: r.hits,
				misses: r.misses,
				hitRate: r.hit_rate,
				evictions: r.evictions,
			};
		}

		async visionCacheWarm(entries: VisionCacheWarmEntry[]): Promise<{ warmed: number; total: number }> {
			const rustEntries = entries.map(e => ({
				content_key: e.contentKey,
				description: e.description,
				model: e.model,
				provider: e.provider,
				processing_time_ms: e.processingTimeMs,
				confidence: e.confidence,
				cached_at: e.cachedAt,
			}));
			const response = await this.request({ command: 'vision/cache-warm', entries: rustEntries });
			if (!response.success) throw new Error(response.error || 'vision/cache-warm failed');
			const r = response.result as { warmed: number; total: number };
			return { warmed: r.warmed, total: r.total };
		}

		async visionCacheEvict(idleMs?: number): Promise<{ evicted: number; remaining: number }> {
			const response = await this.request({ command: 'vision/cache-evict', idle_ms: idleMs });
			if (!response.success) throw new Error(response.error || 'vision/cache-evict failed');
			return response.result as { evicted: number; remaining: number };
		}
	};
}
