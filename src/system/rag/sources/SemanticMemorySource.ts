/**
 * SemanticMemorySource - Tiered memory recall for RAG context
 *
 * Architecture: L1/L2/L3 cache hierarchy (like CPU caches, like the hippocampus).
 * The brain doesn't block cognition while recalling — neither do we.
 *
 * L1 (in-memory, ~0ms) — cached memories from last recall
 * L2 (Rust IPC, ~30ms) — 6-layer parallel recall via Rayon
 * L3 (sentinel, seconds+) — deep research, web queries, cross-persona (future)
 *
 * RAG builds NEVER block on memory recall after the first cold start.
 * L2 refreshes happen in the background. L3 results arrive asynchronously
 * and enrich future responses — like a background thought completing.
 *
 * BATCHING SUPPORT:
 * Still supports batched loading via RAGComposer for the Rust rag/compose path.
 * The tiered cache is used for the direct load() path (TypeScript sources).
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import type { RagSourceRequest, RagSourceResult } from '../../../shared/generated/rag';
import type { PersonaMemory } from '../shared/RAGTypes';
import { TieredMemoryCache } from '../cache/TieredMemoryCache';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('SemanticMemorySource', 'rag');

// Memory tokens are usually dense - estimate higher
const TOKENS_PER_MEMORY_ESTIMATE = 80;

export class SemanticMemorySource implements RAGSource {
  readonly name = 'semantic-memory';
  readonly priority = 60;  // Medium-high - memories inform persona behavior
  readonly defaultBudgetPercent = 12;
  readonly supportsBatching = true;  // Participate in batched Rust IPC

  // Negative cache with exponential backoff: skip IPC when corpus unavailable.
  // Backoff: 2s → 4s → 8s → 16s (cap). Resets on success.
  private static _corpusBackoff: Map<string, { failedAt: number; ttlMs: number }> = new Map();
  private static readonly BACKOFF_MIN_MS = 2_000;
  private static readonly BACKOFF_MAX_MS = 16_000;

  isApplicable(context: RAGSourceContext): boolean {
    const backoff = SemanticMemorySource._corpusBackoff.get(context.personaId);
    if (backoff && (Date.now() - backoff.failedAt) < backoff.ttlMs) {
      return false;
    }
    return true;
  }

  /**
   * Get the batch request for this source.
   * Returns a RagSourceRequest with source_type='memory' for Rust rag/compose.
   */
  getBatchRequest(context: RAGSourceContext, allocatedBudget: number): RagSourceRequest | null {
    const queryText = this.buildSemanticQuery(context);

    return {
      source_type: 'memory',
      budget_tokens: allocatedBudget,
      params: {
        query_text: queryText,
        layers: undefined,  // All layers
      }
    };
  }

  /**
   * Convert Rust RagSourceResult to TypeScript RAGSection.
   * Maps Rust's memory format back to PersonaMemory[].
   */
  fromBatchResult(result: RagSourceResult, loadTimeMs: number): RAGSection {
    // Extract memories from sections
    const memories: PersonaMemory[] = [];

    for (const section of result.sections) {
      // Memory sections have section_type='memory' and contain JSON
      if (section.section_type === 'memory' && section.content) {
        try {
          const mem = JSON.parse(section.content);
          memories.push({
            id: mem.id,
            type: this.mapMemoryType(mem.memory_type || mem.type),
            content: mem.content,
            timestamp: new Date(mem.timestamp),
            relevanceScore: section.relevance ?? mem.importance ?? 0.5
          });
        } catch (e) {
          // If not JSON, treat as raw content
          memories.push({
            id: crypto.randomUUID(),
            type: 'observation',
            content: section.content,
            timestamp: new Date(),
            relevanceScore: section.relevance ?? 0.5
          });
        }
      }
    }

    // Populate L1 cache with batch results so subsequent load() calls are instant
    if (memories.length > 0) {
      // We don't have personaId in this method signature — batch results are
      // handled at composer level. The L1 cache will be populated on next load().
    }

    // Extract metadata from the result
    const metadata = result.metadata as any || {};

    return {
      sourceName: this.name,
      tokenCount: result.tokens_used,
      loadTimeMs,
      memories,
      systemPromptSection: this.formatMemoriesSection(memories),
      metadata: {
        memoryCount: memories.length,
        totalCandidates: metadata.total_candidates,
        rustRecallMs: metadata.recall_time_ms,
        layers: metadata.layers
      }
    };
  }

  async load(context: RAGSourceContext, allocatedBudget: number): Promise<RAGSection> {
    const startTime = performance.now();
    const maxMemories = Math.max(3, Math.floor(allocatedBudget / TOKENS_PER_MEMORY_ESTIMATE));

    try {
      const cache = TieredMemoryCache.instance;
      cache.setLogger((msg) => log.debug(msg));

      const queryText = this.buildSemanticQuery(context);

      // Tiered recall: returns L1 cache instantly, refreshes L2 in background
      const result = await cache.recall({
        personaId: context.personaId,
        roomId: context.roomId,
        queryText,
        maxResults: maxMemories,
      });

      if (result.memories.length === 0) {
        return this.emptySection(startTime);
      }

      // Clear backoff on success — corpus is available
      SemanticMemorySource._corpusBackoff.delete(context.personaId);

      const personaMemories = result.memories.slice(0, maxMemories);
      const loadTimeMs = performance.now() - startTime;
      const tokenCount = personaMemories.reduce((sum, m) => sum + this.estimateTokens(m.content), 0);

      log.debug(`${result.tier} recall: ${personaMemories.length} memories in ${loadTimeMs.toFixed(1)}ms (~${tokenCount} tokens)`);

      return {
        sourceName: this.name,
        tokenCount,
        loadTimeMs,
        memories: personaMemories,
        systemPromptSection: this.formatMemoriesSection(personaMemories),
        metadata: {
          memoryCount: personaMemories.length,
          tier: result.tier,
          cacheLatencyMs: result.latencyMs,
          ...result.metadata,
        }
      };
    } catch (error: any) {
      // Negative-cache "No memory corpus" errors with exponential backoff
      if (error.message?.includes('No memory corpus')) {
        const prev = SemanticMemorySource._corpusBackoff.get(context.personaId);
        const nextTtl = Math.min(
          (prev?.ttlMs ?? SemanticMemorySource.BACKOFF_MIN_MS) * 2,
          SemanticMemorySource.BACKOFF_MAX_MS
        );
        SemanticMemorySource._corpusBackoff.set(context.personaId, { failedAt: Date.now(), ttlMs: nextTtl });
        log.debug(`Corpus unavailable for ${context.personaId.slice(0, 8)}, backoff ${nextTtl}ms`);
      } else {
        log.error(`Failed to load memories: ${error.message}`);
      }
      return this.emptySection(startTime, error.message);
    }
  }

  private buildSemanticQuery(context: RAGSourceContext): string | undefined {
    if (context.options.currentMessage?.content) {
      return context.options.currentMessage.content;
    }
    return context.options.widgetContext?.slice(0, 200);
  }

  private mapMemoryType(type: string): PersonaMemory['type'] {
    const validTypes = ['observation', 'pattern', 'reflection', 'preference', 'goal'];
    return validTypes.includes(type) ? type as PersonaMemory['type'] : 'observation';
  }

  private emptySection(startTime: number, error?: string): RAGSection {
    return {
      sourceName: this.name,
      tokenCount: 0,
      loadTimeMs: performance.now() - startTime,
      memories: [],
      metadata: error ? { error } : {}
    };
  }

  /**
   * Format memories into the system prompt section.
   * This is the SINGLE AUTHORITY for memory formatting.
   */
  private formatMemoriesSection(memories: PersonaMemory[]): string | undefined {
    if (memories.length === 0) return undefined;

    return `\n\n=== YOUR CONSOLIDATED MEMORIES ===\nThese are important things you've learned and consolidated into long-term memory:\n\n${
      memories.map((mem, idx) =>
        `${idx + 1}. [${mem.type}] ${mem.content} (${new Date(mem.timestamp).toLocaleDateString()})`
      ).join('\n')
    }\n\nUse these memories to inform your responses when relevant.\n================================`;
  }

  private estimateTokens(text: string): number {
    return Math.ceil(text.length / 4);
  }
}
