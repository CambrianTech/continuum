/**
 * Unified paging primitive — the foundation that LoRA paging, KV cache
 * paging (PagedAttention), MoE expert paging, embedding cache, and
 * memory recall all build on.
 *
 * See: docs/architecture/UNIFIED-PAGING.md
 */

export {
  PagedResourcePool,
  lruPriority,
  sizeWeightedLruPriority,
  unitSizer,
} from './PagedResourcePool';

export type {
  PagedResourcePoolConfig,
  PoolEntry,
  PoolStats,
  ResourceHandle,
  ResourceLoader,
  ResourceSizer,
  EvictionPriority,
} from './PagedResourcePool';
