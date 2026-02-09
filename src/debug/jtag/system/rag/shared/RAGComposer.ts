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
import type { RagSourceRequest, RagComposeResult, RagSourceResult } from '../../../shared/generated/rag';
import { Logger } from '../../core/logging/Logger';
import { TimingHarness } from '../../core/shared/TimingHarness';

const log = Logger.create('RAGComposer', 'rag');

// ═══════════════════════════════════════════════════════════════════════════
// SHARED IPC CLIENT — Persistent connection to avoid socket congestion
// ═══════════════════════════════════════════════════════════════════════════

import type { RustCoreIPCClient as RustCoreIPCClientType } from '../../../workers/continuum-core/bindings/RustCoreIPC';

let sharedIPCClient: RustCoreIPCClientType | null = null;
let ipcConnecting: Promise<void> | null = null;

/**
 * Get or create a shared IPC client with persistent connection.
 * Prevents socket congestion from multiple personas creating connections.
 */
async function getSharedIPCClient(): Promise<RustCoreIPCClientType> {
  // Already have a connected client - reuse it
  if (sharedIPCClient) {
    return sharedIPCClient;
  }

  // Connection in progress - wait for it
  if (ipcConnecting) {
    await ipcConnecting;
    if (sharedIPCClient) {
      return sharedIPCClient;
    }
  }

  // Create new client and connect
  const { RustCoreIPCClient, getContinuumCoreSocketPath } = await import('../../../workers/continuum-core/bindings/RustCoreIPC');
  const client = new RustCoreIPCClient(getContinuumCoreSocketPath());

  ipcConnecting = client.connect().then(() => {
    sharedIPCClient = client;
    log.info('RAG IPC client connected (shared)');

    // Handle disconnection - clear the shared client so next call reconnects
    client.on('close', () => {
      log.warn('RAG IPC client disconnected - will reconnect on next call');
      if (sharedIPCClient === client) {
        sharedIPCClient = null;
      }
    });

    client.on('error', (err: Error) => {
      log.error(`RAG IPC client error: ${err.message}`);
      if (sharedIPCClient === client) {
        sharedIPCClient = null;
      }
    });
  }).finally(() => {
    ipcConnecting = null;
  });

  await ipcConnecting;
  return client;
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
    const applicableSources: RAGSource[] = [];
    const skippedSources: string[] = [];

    for (const source of this.sources) {
      if (source.isApplicable(context)) {
        applicableSources.push(source);
      } else {
        skippedSources.push(source.name);
      }
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

    // Build the batch promises
    const batchPromise = this.loadBatchedSources(context, batchingSources);
    const typescriptPromises = typescriptSources.map(({ source, budget }) =>
      this.loadTypeScriptSource(source, context, budget)
    );

    // Execute all in parallel
    const [batchResults, ...tsResults] = await Promise.all([batchPromise, ...typescriptPromises]);
    timer.mark('load_sources');

    // Collect batch results
    for (const result of batchResults) {
      if (result.success) {
        sections.push(result.section);
      } else {
        failedSources.push({ source: result.source, error: result.error });
      }
    }

    // Collect TypeScript results
    for (const result of tsResults) {
      if (result.success) {
        sections.push(result.section);
      } else {
        failedSources.push({ source: result.source, error: result.error });
      }
    }
    timer.mark('collect_results');

    // Log slow sources (> 1 second)
    const allLoadTimes = [...batchResults, ...tsResults].filter(r => r.loadTime > 1000);
    if (allLoadTimes.length > 0) {
      log.warn(`Slow RAG sources: ${allLoadTimes.map(s => `${s.success ? s.sourceName : s.source}(${s.loadTime.toFixed(0)}ms)`).join(', ')}`);
    }

    // Calculate totals
    const totalTokens = sections.reduce((sum, s) => sum + s.tokenCount, 0);
    timer.setMeta('totalTokens', totalTokens);
    timer.setMeta('failedSources', failedSources.length);

    const record = timer.finish();
    log.info(`RAG composed: ${sections.length} sections, ${totalTokens} tokens, ${record.totalMs.toFixed(1)}ms (batched=${batchingSources.length}, ts=${typescriptSources.length})`);

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

      // Make ONE IPC call - Rust handles parallel loading via Rayon
      const result: RagComposeResult = await ipc.ragCompose(
        context.personaId,
        context.roomId,
        requests,
        queryText,
        context.totalBudget
      );

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
          if (sourceInfo.source.fromBatchResult) {
            const section = sourceInfo.source.fromBatchResult(rustResult, rustResult.load_time_ms);
            results.push({
              success: true,
              section,
              sourceName: sourceInfo.source.name,
              loadTime: rustResult.load_time_ms
            });
          } else {
            // Fallback: basic conversion
            results.push({
              success: true,
              section: this.defaultFromBatchResult(sourceInfo.source.name, rustResult),
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

      return results;

    } catch (error: any) {
      const record = sourceTimer.finish();
      log.error(`Batch load failed after ${record.totalMs.toFixed(1)}ms: ${error.message}`);

      // If connection error, clear shared client so next call reconnects
      if (error.message?.includes('connect') || error.message?.includes('ENOENT')) {
        sharedIPCClient = null;
      }

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
   */
  private async loadTypeScriptSource(
    source: RAGSource,
    context: RAGSourceContext,
    budget: number
  ): Promise<{ success: true; section: RAGSection; sourceName: string; loadTime: number } |
              { success: false; source: string; error: string; loadTime: number }> {
    const sourceTimer = TimingHarness.start(`rag/source/${source.name}`, 'rag');
    sourceTimer.setMeta('budget', budget);

    try {
      const section = await source.load(context, budget);
      sourceTimer.mark('load');
      sourceTimer.setMeta('tokenCount', section.tokenCount);
      const record = sourceTimer.finish();
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
   */
  private defaultFromBatchResult(sourceName: string, result: RagSourceResult): RAGSection {
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
