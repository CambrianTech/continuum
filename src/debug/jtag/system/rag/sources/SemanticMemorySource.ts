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
 * Previous implementation used TS Hippocampus through the event loop (3-10s under load).
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import type { PersonaMemory } from '../shared/RAGTypes';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('SemanticMemorySource', 'rag');

// Memory tokens are usually dense - estimate higher
const TOKENS_PER_MEMORY_ESTIMATE = 80;

export class SemanticMemorySource implements RAGSource {
  readonly name = 'semantic-memory';
  readonly priority = 60;  // Medium-high - memories inform persona behavior
  readonly defaultBudgetPercent = 15;

  isApplicable(_context: RAGSourceContext): boolean {
    // Always try - will return empty if persona has no memories
    return true;
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
      log.error(`Failed to load memories: ${error.message}`);
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
