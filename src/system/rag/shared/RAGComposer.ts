/**
 * RAGComposer - Orchestrates multiple RAGSources to build complete context
 *
 * Responsibilities:
 * 1. Manage registered sources
 * 2. Allocate token budget across sources
 * 3. Load sources in parallel (batched Rust + TypeScript concurrently)
 * 4. Handle failures gracefully
 * 5. Compose final RAG context
 *
 * Architecture:
 * - Sources are registered once at startup
 * - compose() is called per-request with context
 * - Sources that support batching are loaded via ONE Rust IPC call
 * - TypeScript sources load in parallel with the Rust call
 * - Results are merged respecting priority order
 *
 * Performance:
 * - Before: N sources × M personas = N×M IPC calls (socket congestion)
 * - After: 1 batched Rust call + TypeScript sources in parallel
 * - Rust uses Rayon for parallel source loading (~30ms total vs 1-7s per source)
 */

import type { RAGSource, RAGSourceContext, RAGSection, RAGCompositionResult } from './RAGSource';
import { PromptTier } from './RAGTypes';

/** Sort key for tiers — smaller numbers concatenate first.
 * INVARIANT before SEMI_STABLE before VOLATILE. See PromptTier docs. */
const TIER_ORDER: Record<PromptTier, number> = {
  [PromptTier.INVARIANT]: 0,
  [PromptTier.SEMI_STABLE]: 1,
  [PromptTier.VOLATILE]: 2,
};
import type { RagSourceRequest, RagComposeResult, RagSourceResult } from '../../../shared/generated/rag';
import { Logger } from '../../core/logging/Logger';
import { TimingHarness } from '../../core/shared/TimingHarness';

const log = Logger.create('RAGComposer', 'rag');

// ═══════════════════════════════════════════════════════════════════════════
// SINGLE-FLIGHT COALESCING for shared RAG sources
//
// When 14 personas respond to the same room message simultaneously, 10 of 13
// sources return IDENTICAL data (same conversation history, same docs, same
// project context). Without coalescing: 140 loads. With: 24.
//
// Cache key: `${sourceName}:${roomId}`. TTL: 5 seconds (just long enough for
// all 14 personas to fire). First caller creates the promise, rest piggyback.
// ═══════════════════════════════════════════════════════════════════════════

type SourceResult =
  | { success: true; section: RAGSection; sourceName: string; loadTime: number }
  | { success: false; source: string; error: string; loadTime: number };

interface CachedFlight {
  promise: Promise<SourceResult>;
  createdAt: number;
}

/** Single-flight cache for shared (non-persona-specific) RAG sources */
const sharedSourceFlights = new Map<string, CachedFlight>();

/** How long a cached result is reusable (ms) */
const SHARED_SOURCE_TTL_MS = 5_000;

// ═══════════════════════════════════════════════════════════════════════════
// SHARED IPC CLIENT — Uses the canonical singleton from RustCoreIPCClient
// All consumers (RAG, cognition, ORM) share ONE connection.
// ═══════════════════════════════════════════════════════════════════════════

import type { RustCoreIPCClient as RustCoreIPCClientType } from '../../../workers/continuum-core/bindings/RustCoreIPC';

/**
 * Get the shared IPC client singleton.
 * All 14 personas + RAG composer share ONE Unix socket connection.
 * RequestId correlation handles concurrent requests safely.
 */
async function getSharedIPCClient(): Promise<RustCoreIPCClientType> {
  const { RustCoreIPCClient } = await import('../../../workers/continuum-core/bindings/RustCoreIPC');
  return RustCoreIPCClient.getInstanceAsync();
}

export class RAGComposer {
  private sources: RAGSource[] = [];

  /**
   * Register a RAG source
   * Sources are stored in priority order (highest first)
   */
  register(source: RAGSource): void {
    this.sources.push(source);
    // Sort by priority descending
    this.sources.sort((a, b) => b.priority - a.priority);
    log.debug(`Registered RAG source: ${source.name} (priority=${source.priority}, budget=${source.defaultBudgetPercent}%)`);
  }

  /**
   * Register multiple sources at once
   */
  registerAll(sources: RAGSource[]): void {
    for (const source of sources) {
      this.register(source);
    }
  }

  /**
   * Get all registered sources (for debugging)
   */
  getSources(): readonly RAGSource[] {
    return this.sources;
  }

  /**
   * Compose RAG context from all applicable sources
   *
   * Process:
   * 1. Filter to applicable sources
   * 2. Allocate budget proportionally
   * 3. Separate batching sources (Rust IPC) from TypeScript sources
   * 4. Load batched sources via ONE Rust call + TypeScript sources in parallel
   * 5. Collect results and failures
   *
   * Performance improvement:
   * - Before: N IPC calls per persona per RAG build (1-7s each under load)
   * - After: 1 batched IPC call (~30ms Rust) + parallel TypeScript sources
   *
   * @param context - Source context with persona, room, options
   * @returns Composition result with all sections
   */
  async compose(context: RAGSourceContext): Promise<RAGCompositionResult> {
    const timer = TimingHarness.start('rag/compose', 'rag');
    timer.setMeta('personaId', context.personaId || 'unknown');
    timer.setMeta('roomId', context.roomId || 'unknown');
    timer.setMeta('totalBudget', context.totalBudget);

    // 1. Filter to applicable sources
    // If activeSources is set (from recipe ragTemplate), only those sources are eligible.
    // This is how queue items declare their own RAG requirements.
    const applicableSources: RAGSource[] = [];
    const skippedSources: string[] = [];
    const activeFilter = context.activeSources;

    for (const source of this.sources) {
      // Recipe-driven activation: if activeSources is set, source must be listed
      if (activeFilter && !activeFilter.includes(source.name)) {
        skippedSources.push(source.name);
        continue;
      }
      if (source.isApplicable(context)) {
        applicableSources.push(source);
      } else {
        skippedSources.push(source.name);
      }
    }

    if (activeFilter) {
      log.debug(`RAG recipe filter: ${activeFilter.length} sources requested, ${applicableSources.length} applicable`);
    }
    timer.mark('filter_sources');
    timer.setMeta('applicableSources', applicableSources.length);
    timer.setMeta('skippedSources', skippedSources.length);

    log.debug(`RAG compose: ${applicableSources.length} applicable, ${skippedSources.length} skipped`);

    // 2. Allocate budget proportionally
    const budgetAllocations = this.allocateBudget(applicableSources, context.totalBudget);
    timer.mark('allocate_budget');

    // 3. Separate batching sources from TypeScript-only sources
    const batchingSources: { source: RAGSource; budget: number; request: RagSourceRequest }[] = [];
    const typescriptSources: { source: RAGSource; budget: number }[] = [];

    for (let i = 0; i < applicableSources.length; i++) {
      const source = applicableSources[i];
      const budget = budgetAllocations[i];

      if (source.supportsBatching && source.getBatchRequest) {
        const request = source.getBatchRequest(context, budget);
        if (request) {
          batchingSources.push({ source, budget, request });
        } else {
          // Batching not applicable for this context - load via TypeScript
          typescriptSources.push({ source, budget });
        }
      } else {
        typescriptSources.push({ source, budget });
      }
    }
    timer.mark('partition_sources');
    timer.setMeta('batchingSources', batchingSources.length);
    timer.setMeta('typescriptSources', typescriptSources.length);

    log.debug(`RAG sources: ${batchingSources.length} batched (Rust), ${typescriptSources.length} TypeScript`);

    // 4. Load batched + TypeScript sources in parallel
    const sections: RAGSection[] = [];
    const failedSources: { source: string; error: string }[] = [];

    // Split batching sources into shared (can coalesce) and persona-specific.
    // Shared batching sources piggyback on the first persona's batch result.
    const coalescedBatchPromises: Promise<SourceResult>[] = [];
    const personalBatchingSources: typeof batchingSources = [];

    for (const bs of batchingSources) {
      if (bs.source.isShared && context.roomId) {
        const cacheKey = `batch:${bs.source.name}:${context.roomId}`;
        const existing = sharedSourceFlights.get(cacheKey);

        if (existing && (Date.now() - existing.createdAt) < SHARED_SOURCE_TTL_MS) {
          // Reuse cached batch result for this shared source
          log.debug(`RAG batch coalesce HIT: ${bs.source.name} for room ${context.roomId.slice(0, 8)}`);
          coalescedBatchPromises.push(existing.promise);
          continue;
        }
      }
      personalBatchingSources.push(bs);
    }

    // Build the batch promises (only for sources that weren't coalesced)
    const batchPromise = this.loadBatchedSources(context, personalBatchingSources);
    const typescriptPromises = typescriptSources.map(({ source, budget }) =>
      this.loadTypeScriptSource(source, context, budget)
    );

    // Per-source timeout: if any RAG source hangs, the entire Promise.all
    // hangs, which wedges every persona's evaluator pipeline (caught in
    // production 2026-04-19 — Joel's chat-validate session showed personas
    // never reaching respondToMessage because compose() never returned).
    // 30s is generous (most sources <50ms); a hung source returns SourceResult
    // failure marker instead of blocking the whole composition. Failed
    // sources show up in failedSources for diagnosis.
    const PER_SOURCE_TIMEOUT_MS = 30_000;
    const withTimeout = <T extends SourceResult | SourceResult[]>(
      promise: Promise<T>,
      sourceName: string,
    ): Promise<T> => {
      let timer: ReturnType<typeof setTimeout> | null = null;
      const timeoutPromise = new Promise<T>((_, reject) => {
        timer = setTimeout(() => {
          reject(new Error(`RAG source '${sourceName}' timed out after ${PER_SOURCE_TIMEOUT_MS}ms`));
        }, PER_SOURCE_TIMEOUT_MS);
      });
      return Promise.race([promise, timeoutPromise]).finally(() => {
        if (timer) clearTimeout(timer);
      }) as Promise<T>;
    };

    // Wrap each individual TS source promise. Catch timeouts so they
    // become SourceResult failures rather than rejecting the Promise.all.
    const guardedTsPromises = typescriptSources.map(({ source }, i) =>
      withTimeout(typescriptPromises[i], source.name).catch(err => ({
        success: false as const,
        source: source.name,
        error: err instanceof Error ? err.message : String(err),
        loadTime: PER_SOURCE_TIMEOUT_MS,
      } satisfies SourceResult))
    );
    const guardedBatchPromise = withTimeout(batchPromise, 'batched-sources').catch(err => {
      log.warn(`Batched RAG sources timed out: ${err}`);
      return [] as SourceResult[];
    });
    const guardedCoalescedPromises = coalescedBatchPromises.map((p, i) =>
      withTimeout(p, `coalesced-${i}`).catch(err => ({
        success: false as const,
        source: `coalesced-${i}`,
        error: err instanceof Error ? err.message : String(err),
        loadTime: PER_SOURCE_TIMEOUT_MS,
      } satisfies SourceResult))
    );

    // Execute all in parallel: batch array + individual coalesced + individual TS sources
    const [batchResults, ...individualResults] = await Promise.all([
      guardedBatchPromise,
      ...guardedCoalescedPromises,
      ...guardedTsPromises,
    ]);
    timer.mark('load_sources');

    // Collect all results into sections/failures
    const allLoadResults: SourceResult[] = [...batchResults, ...individualResults];
    for (const result of allLoadResults) {
      if (result.success) {
        sections.push(result.section);
      } else {
        failedSources.push({ source: result.source, error: result.error });
      }
    }
    // Deterministic ordering: sections sorted by (tier, sourceName) so the
    // assembled prompt's bytes are identical across requests with identical
    // section contents. This is the prerequisite for llama-server / DMR
    // prefix-KV-cache reuse — without stable ordering, the same logical
    // prompt has different bytes per turn and the cache misses every time.
    // Tier order: INVARIANT first, then SEMI_STABLE, then VOLATILE — see
    // PromptTier in RAGTypes.ts and docs/architecture/MULTIMODAL-WORKER-AND-PREFIX-REUSE.md
    sections.sort((a, b) => {
      const tierOrder = TIER_ORDER[a.tier] - TIER_ORDER[b.tier];
      if (tierOrder !== 0) return tierOrder;
      // Within a tier: alphabetical by source name. Stable, total order.
      return a.sourceName.localeCompare(b.sourceName);
    });
    timer.mark('collect_results');

    // Log ALL source timings for performance diagnosis
    const allResults = allLoadResults;
    const sortedByTime = allResults.slice().sort((a, b) => b.loadTime - a.loadTime);
    const sourceTimingSummary = sortedByTime
      .map(s => `${s.success ? s.sourceName : s.source}:${s.loadTime.toFixed(0)}ms`)
      .join(', ');

    // Calculate totals
    const totalTokens = sections.reduce((sum, s) => sum + s.tokenCount, 0);
    timer.setMeta('totalTokens', totalTokens);
    timer.setMeta('failedSources', failedSources.length);

    const record = timer.finish();
    // Always log source breakdown — critical for finding the gating source in parallel loads
    log.info(`RAG composed: ${sections.length} sections, ${totalTokens} tokens, ${record.totalMs.toFixed(1)}ms | sources: ${sourceTimingSummary}`);

    return {
      sections,
      totalTokens,
      totalLoadTimeMs: record.totalMs,
      skippedSources,
      failedSources
    };
  }

  /**
   * Load multiple sources via ONE batched Rust IPC call.
   * Uses Rayon parallelism on the Rust side for true concurrency.
   */
  private async loadBatchedSources(
    context: RAGSourceContext,
    batchingSources: { source: RAGSource; budget: number; request: RagSourceRequest }[]
  ): Promise<Array<{ success: true; section: RAGSection; sourceName: string; loadTime: number } |
                   { success: false; source: string; error: string; loadTime: number }>> {
    if (batchingSources.length === 0) {
      return [];
    }

    const sourceTimer = TimingHarness.start('rag/batch', 'rag');
    sourceTimer.setMeta('sourceCount', batchingSources.length);

    try {
      // Use shared persistent connection instead of per-request connection
      const ipc = await getSharedIPCClient();

      // Build request array
      const requests = batchingSources.map(bs => bs.request);

      // Build query text from current message
      const queryText = context.options.currentMessage?.content;

      // Make ONE IPC call - Rust handles parallel loading via Rayon.
      // Same 30s watchdog as per-source path — a hung Rust batch blocks
      // every downstream persona call just as effectively as a hung
      // TS source, so treat it with the same discipline.
      const BATCH_TIMEOUT_MS = 30_000;
      const batchWatchdog = new Promise<never>((_, reject) => {
        const t = setTimeout(() => {
          reject(new Error(`RAG batch IPC timed out after ${BATCH_TIMEOUT_MS}ms`));
        }, BATCH_TIMEOUT_MS);
        t.unref?.();
      });
      const result: RagComposeResult = await Promise.race([
        ipc.ragCompose(
          context.personaId,
          context.roomId,
          requests,
          queryText,
          context.totalBudget
        ),
        batchWatchdog,
      ]);

      sourceTimer.mark('ipc_call');
      sourceTimer.setMeta('rustComposeMs', result.compose_time_ms);
      sourceTimer.setMeta('sourcesSucceeded', result.sources_succeeded);
      sourceTimer.setMeta('sourcesFailed', result.sources_failed);

      const record = sourceTimer.finish();

      // Map Rust results back to TypeScript RAGSection format
      const results: Array<{ success: true; section: RAGSection; sourceName: string; loadTime: number } |
                          { success: false; source: string; error: string; loadTime: number }> = [];

      for (let i = 0; i < result.source_results.length; i++) {
        const rustResult = result.source_results[i];
        const sourceInfo = batchingSources.find(bs =>
          bs.request.source_type === rustResult.source_type
        );

        if (!sourceInfo) {
          log.warn(`No source found for result type ${rustResult.source_type}`);
          continue;
        }

        if (rustResult.success) {
          // Convert via source's fromBatchResult method
          // Tier injection: same single-source-of-truth as TS path —
          // the source's class declaration provides tier; we inject it
          // here so the section conforms to RAGSection regardless of
          // whether the source's fromBatchResult included it.
          if (sourceInfo.source.fromBatchResult) {
            const rawSection = sourceInfo.source.fromBatchResult(rustResult, rustResult.load_time_ms);
            const section: RAGSection = { ...rawSection, tier: sourceInfo.source.tier };
            results.push({
              success: true,
              section,
              sourceName: sourceInfo.source.name,
              loadTime: rustResult.load_time_ms
            });
          } else {
            // Fallback: basic conversion
            const rawSection = this.defaultFromBatchResult(sourceInfo.source.name, rustResult);
            const section: RAGSection = { ...rawSection, tier: sourceInfo.source.tier };
            results.push({
              success: true,
              section,
              sourceName: sourceInfo.source.name,
              loadTime: rustResult.load_time_ms
            });
          }
        } else {
          results.push({
            success: false,
            source: sourceInfo.source.name,
            error: rustResult.error || 'Unknown batch error',
            loadTime: rustResult.load_time_ms
          });
        }
      }

      log.debug(`Batched ${batchingSources.length} sources in ${record.totalMs.toFixed(1)}ms (Rust: ${result.compose_time_ms.toFixed(1)}ms)`);

      // Cache shared batch results for subsequent personas in the same room
      for (const r of results) {
        if (!r.success) continue;
        const matchingSource = batchingSources.find(bs => bs.source.name === r.sourceName);
        if (matchingSource?.source.isShared && context.roomId) {
          const cacheKey = `batch:${r.sourceName}:${context.roomId}`;
          sharedSourceFlights.set(cacheKey, {
            promise: Promise.resolve(r),
            createdAt: Date.now(),
          });
        }
      }

      // Cleanup stale entries — same logic as TypeScript source path
      if (sharedSourceFlights.size > 30) {
        const now = Date.now();
        for (const [key, entry] of sharedSourceFlights) {
          if (now - entry.createdAt > SHARED_SOURCE_TTL_MS) {
            sharedSourceFlights.delete(key);
          }
        }
      }

      return results;

    } catch (error: any) {
      const record = sourceTimer.finish();
      log.error(`Batch load failed after ${record.totalMs.toFixed(1)}ms: ${error.message}`);

      // Return failures for all batched sources
      return batchingSources.map(bs => ({
        success: false as const,
        source: bs.source.name,
        error: error.message,
        loadTime: record.totalMs
      }));
    }
    // NOTE: Don't disconnect - shared client stays connected for reuse
  }

  /**
   * Load a single TypeScript source (non-batching).
   *
   * For shared sources (isShared=true), uses single-flight coalescing:
   * when 14 personas compose RAG for the same room, only the first
   * actually loads the source — the other 13 reuse the same promise.
   */
  private async loadTypeScriptSource(
    source: RAGSource,
    context: RAGSourceContext,
    budget: number
  ): Promise<SourceResult> {
    // Shared source coalescing: reuse in-flight or recently-completed result
    if (source.isShared && context.roomId) {
      const cacheKey = `${source.name}:${context.roomId}`;
      const existing = sharedSourceFlights.get(cacheKey);

      if (existing && (Date.now() - existing.createdAt) < SHARED_SOURCE_TTL_MS) {
        // Piggyback on existing flight — no new load
        log.debug(`RAG coalesce HIT: ${source.name} for room ${context.roomId.slice(0, 8)} (persona ${context.personaId.slice(0, 8)})`);
        return existing.promise;
      }

      // First caller — create the flight
      const flight = this.doLoadTypeScriptSource(source, context, budget);
      sharedSourceFlights.set(cacheKey, { promise: flight, createdAt: Date.now() });

      // Cleanup stale entries periodically
      if (sharedSourceFlights.size > 30) {
        const now = Date.now();
        for (const [key, entry] of sharedSourceFlights) {
          if (now - entry.createdAt > SHARED_SOURCE_TTL_MS) {
            sharedSourceFlights.delete(key);
          }
        }
      }

      return flight;
    }

    // Persona-specific source: always load fresh
    return this.doLoadTypeScriptSource(source, context, budget);
  }

  /**
   * Actually load a TypeScript source (called by loadTypeScriptSource).
   */
  private async doLoadTypeScriptSource(
    source: RAGSource,
    context: RAGSourceContext,
    budget: number
  ): Promise<SourceResult> {
    const sourceTimer = TimingHarness.start(`rag/source/${source.name}`, 'rag');
    sourceTimer.setMeta('budget', budget);

    // Per-source watchdog. Without this, one hanging source blocks the
    // Promise.all in compose() and every persona chat-path stalls. Timing
    // out a source and marking it failed is strictly better than an
    // unresponsive chat pipeline. Default 30s — generous enough for cold
    // memory-recall queries, tight enough that a truly broken source gets
    // reported rather than hiding as silence.
    const SOURCE_TIMEOUT_MS = 30_000;
    const watchdog = new Promise<never>((_, reject) => {
      const t = setTimeout(() => {
        reject(new Error(`RAG source '${source.name}' timed out after ${SOURCE_TIMEOUT_MS}ms`));
      }, SOURCE_TIMEOUT_MS);
      // Prevent the timer from keeping the event loop alive past the
      // response it's guarding. Node's default is already .ref()d; unref
      // lets the process exit cleanly if all else finishes.
      t.unref?.();
    });

    try {
      const rawSection = await Promise.race([source.load(context, budget), watchdog]);
      sourceTimer.mark('load');
      sourceTimer.setMeta('tokenCount', rawSection.tokenCount);
      const record = sourceTimer.finish();
      // Inject tier from the source's declaration. Sources don't re-state
      // their tier on every return; the class-level declaration is the
      // single source of truth, applied here.
      const section: RAGSection = { ...rawSection, tier: source.tier };
      return { success: true, section, sourceName: source.name, loadTime: record.totalMs };
    } catch (error: any) {
      sourceTimer.setError(error.message);
      const record = sourceTimer.finish();
      log.error(`RAG source ${source.name} failed after ${record.totalMs.toFixed(1)}ms: ${error.message}`);
      return { success: false, source: source.name, error: error.message, loadTime: record.totalMs };
    }
  }

  /**
   * Default conversion from Rust RagSourceResult to TypeScript RAGSection.
   * Used when source doesn't implement fromBatchResult.
   * Returns without `tier` — caller injects from the source's declaration.
   */
  private defaultFromBatchResult(sourceName: string, result: RagSourceResult): Omit<RAGSection, 'tier'> {
    // Combine all sections into a single content block
    const content = result.sections
      .map(s => s.content)
      .filter(Boolean)
      .join('\n\n');

    return {
      sourceName,
      tokenCount: result.tokens_used,
      loadTimeMs: result.load_time_ms,
      systemPromptSection: content || undefined,
      metadata: result.metadata
    };
  }

  /**
   * Allocate token budget across sources
   *
   * Algorithm:
   * 1. Sum up default percentages
   * 2. Normalize if total != 100
   * 3. Allocate proportionally to total budget
   * 4. Ensure minimum allocation for each source
   */
  private allocateBudget(sources: RAGSource[], totalBudget: number): number[] {
    if (sources.length === 0) return [];

    // Sum default percentages
    const totalPercent = sources.reduce((sum, s) => sum + s.defaultBudgetPercent, 0);

    // Allocate proportionally
    const allocations = sources.map(source => {
      const normalized = totalPercent > 0
        ? source.defaultBudgetPercent / totalPercent
        : 1 / sources.length;
      return Math.floor(totalBudget * normalized);
    });

    // Ensure minimum of 100 tokens per source
    const MIN_TOKENS = 100;
    for (let i = 0; i < allocations.length; i++) {
      if (allocations[i] < MIN_TOKENS) {
        allocations[i] = MIN_TOKENS;
      }
    }

    return allocations;
  }
}

/**
 * Singleton instance for app-wide use
 */
let defaultComposer: RAGComposer | null = null;

export function getRAGComposer(): RAGComposer {
  if (!defaultComposer) {
    defaultComposer = new RAGComposer();
  }
  return defaultComposer;
}

/**
 * Reset singleton (for testing)
 */
export function resetRAGComposer(): void {
  defaultComposer = null;
}
