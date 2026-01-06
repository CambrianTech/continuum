/**
 * RAGComposer - Orchestrates multiple RAGSources to build complete context
 *
 * Responsibilities:
 * 1. Manage registered sources
 * 2. Allocate token budget across sources
 * 3. Load sources in parallel
 * 4. Handle failures gracefully
 * 5. Compose final RAG context
 *
 * Architecture:
 * - Sources are registered once at startup
 * - compose() is called per-request with context
 * - Sources load in parallel (Promise.all)
 * - Results are merged respecting priority order
 */

import type { RAGSource, RAGSourceContext, RAGSection, RAGCompositionResult } from './RAGSource';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('RAGComposer', 'rag');

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
   * 3. Load all sources in parallel
   * 4. Collect results and failures
   *
   * @param context - Source context with persona, room, options
   * @returns Composition result with all sections
   */
  async compose(context: RAGSourceContext): Promise<RAGCompositionResult> {
    const startTime = performance.now();

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

    log.debug(`RAG compose: ${applicableSources.length} applicable, ${skippedSources.length} skipped`);

    // 2. Allocate budget proportionally
    const budgetAllocations = this.allocateBudget(applicableSources, context.totalBudget);

    // 3. Load all sources in parallel (with per-source timing)
    const loadPromises = applicableSources.map(async (source, index) => {
      const allocated = budgetAllocations[index];
      const sourceStartTime = performance.now();
      try {
        const section = await source.load(context, allocated);
        const sourceLoadTime = performance.now() - sourceStartTime;
        log.debug(`Source ${source.name} loaded in ${sourceLoadTime.toFixed(1)}ms`);
        return { success: true as const, section, sourceName: source.name, loadTime: sourceLoadTime };
      } catch (error: any) {
        const sourceLoadTime = performance.now() - sourceStartTime;
        log.error(`RAG source ${source.name} failed after ${sourceLoadTime.toFixed(1)}ms: ${error.message}`);
        return { success: false as const, source: source.name, error: error.message, loadTime: sourceLoadTime };
      }
    });

    const results = await Promise.all(loadPromises);

    // Log slow sources (> 1 second)
    const slowSources = results.filter(r => r.loadTime > 1000);
    if (slowSources.length > 0) {
      log.warn(`Slow RAG sources: ${slowSources.map(s => `${s.success ? s.sourceName : s.source}(${s.loadTime.toFixed(0)}ms)`).join(', ')}`);
    }

    // 4. Collect results
    const sections: RAGSection[] = [];
    const failedSources: { source: string; error: string }[] = [];

    for (const result of results) {
      if (result.success) {
        sections.push(result.section);
      } else {
        failedSources.push({ source: result.source, error: result.error });
      }
    }

    // Calculate totals
    const totalTokens = sections.reduce((sum, s) => sum + s.tokenCount, 0);
    const totalLoadTimeMs = performance.now() - startTime;

    log.info(`RAG composed: ${sections.length} sections, ${totalTokens} tokens, ${totalLoadTimeMs.toFixed(1)}ms`);

    return {
      sections,
      totalTokens,
      totalLoadTimeMs,
      skippedSources,
      failedSources
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
