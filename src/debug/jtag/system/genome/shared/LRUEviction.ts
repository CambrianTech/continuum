/**
 * LRU Eviction Algorithm - Pure Functions
 *
 * Selects which adapters to evict when memory is needed.
 * Uses weighted LRU: age + priority + usage frequency.
 *
 * Phase 7: Simple LRU with priority weighting
 * Phase 8+: Add usage frequency, domain importance
 */

import type { MockLoRAAdapter } from './MockLoRAAdapter';

/**
 * Eviction candidate with score
 */
export interface EvictionCandidate {
  adapter: MockLoRAAdapter;
  score: number;  // Higher = more likely to evict
}

/**
 * Calculate eviction candidates from list of loaded adapters
 *
 * Returns adapters sorted by eviction score (highest first).
 * Pure function - no side effects.
 *
 * @param loadedAdapters All currently loaded adapters
 * @returns Candidates sorted by score (descending)
 */
export function calculateEvictionCandidates(
  loadedAdapters: MockLoRAAdapter[]
): EvictionCandidate[] {
  // Calculate score for each adapter
  const candidates: EvictionCandidate[] = loadedAdapters.map(adapter => ({
    adapter,
    score: adapter.calculateEvictionScore()
  }));

  // Filter out non-evictable (score = -Infinity)
  const evictable = candidates.filter(c => c.score !== -Infinity);

  // Sort by score (highest first = evict first)
  evictable.sort((a, b) => b.score - a.score);

  return evictable;
}

/**
 * Select adapters to evict to free up target memory
 *
 * Greedily selects highest-scored adapters until target memory freed.
 * Pure function - doesn't actually evict, just returns list.
 *
 * @param loadedAdapters All currently loaded adapters
 * @param targetFreeMB How much memory we need to free
 * @returns List of adapters to evict (ordered by score)
 */
export function selectAdaptersToEvict(
  loadedAdapters: MockLoRAAdapter[],
  targetFreeMB: number
): MockLoRAAdapter[] {
  if (targetFreeMB <= 0) {
    return [];
  }

  const candidates = calculateEvictionCandidates(loadedAdapters);

  const toEvict: MockLoRAAdapter[] = [];
  let freedMB = 0;

  for (const candidate of candidates) {
    toEvict.push(candidate.adapter);
    freedMB += candidate.adapter.getSize();

    if (freedMB >= targetFreeMB) {
      break;  // We've freed enough memory
    }
  }

  return toEvict;
}

/**
 * Check if evicting specific adapter would cause thrashing
 *
 * Thrashing = evicting adapter that was recently loaded.
 * Hysteresis: Don't evict adapters loaded in last N seconds.
 *
 * @param adapter Adapter to check
 * @param hysteresisSeconds Min time since load (default 60s)
 * @returns True if evicting would cause thrashing
 */
export function wouldCauseThrashing(
  adapter: MockLoRAAdapter,
  hysteresisSeconds: number = 60
): boolean {
  const loadedAt = adapter.getLoadedAt();
  if (!loadedAt) {
    return false;  // Not loaded, can't thrash
  }

  const ageSeconds = (Date.now() - loadedAt) / 1000;
  return ageSeconds < hysteresisSeconds;
}

/**
 * Filter out adapters that would cause thrashing
 *
 * Removes recently-loaded adapters from eviction list.
 *
 * @param adapters Adapters to filter
 * @param hysteresisSeconds Min time since load
 * @returns Filtered list (safe to evict)
 */
export function filterThrashingAdapters(
  adapters: MockLoRAAdapter[],
  hysteresisSeconds: number = 60
): MockLoRAAdapter[] {
  return adapters.filter(adapter => !wouldCauseThrashing(adapter, hysteresisSeconds));
}

/**
 * Smart eviction with thrashing protection
 *
 * Combines LRU selection with hysteresis to prevent thrashing.
 * If we can't free enough memory without thrashing, returns partial list.
 *
 * @param loadedAdapters All currently loaded adapters
 * @param targetFreeMB How much memory we need to free
 * @param hysteresisSeconds Min time since load (default 60s)
 * @returns Adapters to evict (may be less than needed if thrashing)
 */
export function selectAdaptersWithThrashingProtection(
  loadedAdapters: MockLoRAAdapter[],
  targetFreeMB: number,
  hysteresisSeconds: number = 60
): { toEvict: MockLoRAAdapter[]; freedMB: number; wouldThrash: boolean } {
  // Get LRU-sorted candidates
  const candidates = calculateEvictionCandidates(loadedAdapters);

  const toEvict: MockLoRAAdapter[] = [];
  let freedMB = 0;

  for (const candidate of candidates) {
    // Skip if evicting would cause thrashing
    if (wouldCauseThrashing(candidate.adapter, hysteresisSeconds)) {
      continue;
    }

    toEvict.push(candidate.adapter);
    freedMB += candidate.adapter.getSize();

    if (freedMB >= targetFreeMB) {
      // We freed enough without thrashing
      return { toEvict, freedMB, wouldThrash: false };
    }
  }

  // We ran out of safe candidates - would need to thrash to free more
  const wouldThrash = freedMB < targetFreeMB;

  return { toEvict, freedMB, wouldThrash };
}
