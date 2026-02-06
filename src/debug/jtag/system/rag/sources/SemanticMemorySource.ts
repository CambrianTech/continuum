/**
 * SemanticMemorySource - Loads private memories for RAG context via Rust IPC
 *
 * Delegates to Rust's 6-layer multi-recall algorithm which runs in parallel:
 *   Core (importance >= 0.8) | Semantic (embedding cosine similarity)
 *   Temporal (recent 2h)     | Associative (tag/relatedTo graph)
 *   Decay Resurface (spaced repetition) | Cross-Context (other rooms)
 *
 * All layers execute concurrently via Rayon in ~30ms total,
 * bypassing the Node.js event loop entirely.
 *
 * BATCHING SUPPORT:
 * This source implements supportsBatching=true to participate in RAGComposer's
 * batched loading. Instead of making individual IPC calls, it provides a
 * RagSourceRequest that's combined with other batching sources into ONE Rust call.
 *
 * Performance:
 * - Before: 1 IPC call per persona per RAG build (1-7s under socket congestion)
 * - After: Combined into single rag/compose call (~30ms total for all sources)
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import type { RagSourceRequest, RagSourceResult } from '../../../shared/generated/rag';
import type { PersonaMemory } from '../shared/RAGTypes';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('SemanticMemorySource', 'rag');

// Memory tokens are usually dense - estimate higher
const TOKENS_PER_MEMORY_ESTIMATE = 80;

export class SemanticMemorySource implements RAGSource {
  readonly name = 'semantic-memory';
  readonly priority = 60;  // Medium-high - memories inform persona behavior
  readonly defaultBudgetPercent = 15;
  readonly supportsBatching = true;  // Participate in batched Rust IPC

  // Negative cache: when Rust returns "No memory corpus", skip IPC for 60s.
  // Without this, each failing persona makes a 1-3s IPC call every RAG build.
  private static _corpusUnavailable: Map<string, number> = new Map();
  private static readonly NEGATIVE_CACHE_TTL_MS = 60_000;

  isApplicable(context: RAGSourceContext): boolean {
    // Skip if we recently learned this persona's corpus is unavailable
    const failedAt = SemanticMemorySource._corpusUnavailable.get(context.personaId);
    if (failedAt && (Date.now() - failedAt) < SemanticMemorySource.NEGATIVE_CACHE_TTL_MS) {
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

    // Extract metadata from the result
    const metadata = result.metadata as any || {};

    return {
      sourceName: this.name,
      tokenCount: result.tokens_used,
      loadTimeMs,
      memories,
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
      // Get PersonaUser to access Rust bridge
      const { UserDaemonServer } = await import('../../../daemons/user-daemon/server/UserDaemonServer');
      const userDaemon = UserDaemonServer.getInstance();

      if (!userDaemon) {
        log.debug('UserDaemon not available, skipping memories');
        return this.emptySection(startTime);
      }

      const personaUser = userDaemon.getPersonaUser(context.personaId);
      if (!personaUser) {
        return this.emptySection(startTime);
      }

      // Access the Rust cognition bridge (nullable getter — no throw)
      const bridge = (personaUser as any).rustCognitionBridge;
      if (!bridge) {
        log.debug('Rust cognition bridge not available, skipping memories');
        return this.emptySection(startTime);
      }

      // Build semantic query from current message
      const queryText = this.buildSemanticQuery(context);

      // Single IPC call → Rust runs all 6 recall layers in parallel via Rayon
      const result = await bridge.memoryMultiLayerRecall({
        query_text: queryText ?? null,
        room_id: context.roomId,
        max_results: maxMemories,
        layers: null,  // All layers
      });

      if (result.memories.length === 0) {
        return this.emptySection(startTime);
      }

      // Convert Rust MemoryRecord to PersonaMemory format
      const personaMemories: PersonaMemory[] = result.memories.map((mem: any) => ({
        id: mem.id,
        type: this.mapMemoryType(mem.memory_type),
        content: mem.content,
        timestamp: new Date(mem.timestamp),
        relevanceScore: mem.relevance_score ?? mem.importance
      }));

      const loadTimeMs = performance.now() - startTime;
      const tokenCount = personaMemories.reduce((sum, m) => sum + this.estimateTokens(m.content), 0);

      // Log layer-level detail for performance monitoring
      const layerSummary = result.layer_timings
        .map((l: any) => `${l.layer}(${l.results_found})`)
        .join(', ');
      log.debug(`Loaded ${personaMemories.length} memories in ${loadTimeMs.toFixed(1)}ms (~${tokenCount} tokens) layers=[${layerSummary}] rust=${result.recall_time_ms.toFixed(1)}ms`);

      return {
        sourceName: this.name,
        tokenCount,
        loadTimeMs,
        memories: personaMemories,
        metadata: {
          memoryCount: personaMemories.length,
          totalCandidates: result.total_candidates,
          rustRecallMs: result.recall_time_ms,
          semanticQuery: queryText?.slice(0, 50),
          layers: layerSummary
        }
      };
    } catch (error: any) {
      // Negative-cache "No memory corpus" errors — skip IPC for 60s
      if (error.message?.includes('No memory corpus')) {
        SemanticMemorySource._corpusUnavailable.set(context.personaId, Date.now());
        log.debug(`Corpus unavailable for ${context.personaId.slice(0, 8)}, negative-cached for 60s`);
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

  private estimateTokens(text: string): number {
    return Math.ceil(text.length / 4);
  }
}
