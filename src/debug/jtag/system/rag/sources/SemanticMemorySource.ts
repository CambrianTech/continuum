/**
 * SemanticMemorySource - Loads private memories for RAG context
 *
 * Features:
 * - Core memory recall (high-importance learnings that should never be forgotten)
 * - Semantic recall (contextually relevant memories based on query)
 * - Deduplication between core and semantic results
 * - Graceful fallback if no semantic query provided
 *
 * PERFORMANCE: Caches core memories per persona for 30 seconds to reduce DB load.
 * Semantic recall is NOT cached since it depends on the specific query.
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import type { PersonaMemory } from '../shared/RAGTypes';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('SemanticMemorySource', 'rag');

// Memory tokens are usually dense - estimate higher
const TOKENS_PER_MEMORY_ESTIMATE = 80;

/**
 * Cache entry for core memories
 */
interface CoreMemoriesCache {
  memories: any[];
  cachedAt: number;
}

/**
 * Cache TTL for core memories (30 seconds)
 * Core memories are high-importance and don't change frequently
 */
const CORE_MEMORY_CACHE_TTL_MS = 30_000;

/**
 * Cache for core memories by personaId
 */
const coreMemoriesCache = new Map<string, CoreMemoriesCache>();

/**
 * Clear expired core memory cache entries
 */
function clearExpiredCoreMemoryCache(): void {
  const now = Date.now();
  for (const [key, entry] of coreMemoriesCache) {
    if (now - entry.cachedAt > CORE_MEMORY_CACHE_TTL_MS) {
      coreMemoriesCache.delete(key);
    }
  }
}

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
      // Get UserDaemon to access PersonaUser
      const { UserDaemonServer } = await import('../../../daemons/user-daemon/server/UserDaemonServer');
      const userDaemon = UserDaemonServer.getInstance();

      if (!userDaemon) {
        log.debug('UserDaemon not available, skipping memories');
        return this.emptySection(startTime);
      }

      // Get PersonaUser instance
      const personaUser = userDaemon.getPersonaUser(context.personaId);
      if (!personaUser) {
        // Not a PersonaUser (humans, agents don't have memories)
        return this.emptySection(startTime);
      }

      // Check for recallMemories capability (duck-typing)
      if (!('recallMemories' in personaUser) || typeof (personaUser as any).recallMemories !== 'function') {
        return this.emptySection(startTime);
      }

      const recallableUser = personaUser as {
        recallMemories: (params: any) => Promise<any[]>;
        semanticRecallMemories?: (query: string, params: any) => Promise<any[]>;
      };

      let memories: any[] = [];
      let coreMemoryCount = 0;
      const RECALL_TIMEOUT_MS = 3000;  // 3 second timeout for any memory operation
      const now = Date.now();

      // Clear expired cache entries periodically
      if (coreMemoriesCache.size > 50) {
        clearExpiredCoreMemoryCache();
      }

      // 1. ALWAYS fetch core memories first (high-importance learnings)
      // These are tool usage learnings, key insights, etc. that should never be forgotten
      // Check cache first to reduce DB load
      const cached = coreMemoriesCache.get(context.personaId);
      if (cached && now - cached.cachedAt < CORE_MEMORY_CACHE_TTL_MS) {
        // Cache hit for core memories (no logging to avoid overhead)
        if (cached.memories.length > 0) {
          memories = [...cached.memories];
          coreMemoryCount = cached.memories.length;
        }
      } else {
        // Cache miss - fetch from DB
        try {
          const corePromise = recallableUser.recallMemories({
            minImportance: 0.8,
            limit: Math.min(3, maxMemories),
            since: new Date(Date.now() - 30 * 24 * 60 * 60 * 1000).toISOString()
          });
          const coreTimeout = new Promise<any[]>((_, reject) =>
            setTimeout(() => reject(new Error('Core memory recall timeout')), RECALL_TIMEOUT_MS)
          );
          const coreMemories = await Promise.race([corePromise, coreTimeout]);

          // Cache the result
          coreMemoriesCache.set(context.personaId, {
            memories: coreMemories,
            cachedAt: now
          });

          if (coreMemories.length > 0) {
            log.debug(`Core memories loaded: ${coreMemories.length} (importance >= 0.8)`);
            memories = [...coreMemories];
            coreMemoryCount = coreMemories.length;
          }
        } catch (e: any) {
          log.warn(`Core memory recall failed (${e.message}), continuing without core memories`);
        }
      }

      // 2. Semantic recall if query available (with timeout to prevent blocking)
      // Build semantic query from recent messages (options.currentMessage or last message context)
      const remainingSlots = maxMemories - memories.length;
      const semanticQuery = this.buildSemanticQuery(context);

      if (remainingSlots > 0 && semanticQuery && semanticQuery.length > 10) {
        if (recallableUser.semanticRecallMemories) {
          try {
            // Timeout after 3 seconds - embedding generation can hang
            const SEMANTIC_TIMEOUT_MS = 3000;
            const semanticPromise = recallableUser.semanticRecallMemories(semanticQuery, {
              limit: remainingSlots,
              semanticThreshold: 0.5,
              minImportance: 0.4
            });

            const timeoutPromise = new Promise<any[]>((_, reject) =>
              setTimeout(() => reject(new Error('Semantic recall timeout')), SEMANTIC_TIMEOUT_MS)
            );

            const semanticMemories = await Promise.race([semanticPromise, timeoutPromise]);

            // Dedupe by id
            const seenIds = new Set(memories.map((m: any) => m.id));
            for (const mem of semanticMemories) {
              if (!seenIds.has(mem.id)) {
                memories.push(mem);
                seenIds.add(mem.id);
              }
            }

            log.debug(`Semantic recall: "${semanticQuery.slice(0, 30)}..." → ${semanticMemories.length} found`);
          } catch (e: any) {
            // Timeout or error - fall back to core memories only
            log.warn(`Semantic recall failed (${e.message}), using core memories only`);
          }
        }
      }

      // 3. Fallback if still empty (also with timeout)
      if (memories.length === 0) {
        try {
          const fallbackPromise = recallableUser.recallMemories({
            minImportance: 0.6,
            limit: maxMemories,
            since: new Date(Date.now() - 7 * 24 * 60 * 60 * 1000).toISOString()
          });
          const fallbackTimeout = new Promise<any[]>((_, reject) =>
            setTimeout(() => reject(new Error('Fallback memory recall timeout')), RECALL_TIMEOUT_MS)
          );
          memories = await Promise.race([fallbackPromise, fallbackTimeout]);
        } catch (e: any) {
          log.warn(`Fallback memory recall failed (${e.message})`);
        }
      }

      if (memories.length === 0) {
        return this.emptySection(startTime);
      }

      // Convert to PersonaMemory format
      const personaMemories: PersonaMemory[] = memories.map((mem: any) => ({
        id: mem.id,
        type: this.mapMemoryType(mem.type),
        content: mem.content,
        timestamp: new Date(mem.timestamp),
        relevanceScore: mem.importance
      }));

      const loadTimeMs = performance.now() - startTime;
      const tokenCount = personaMemories.reduce((sum, m) => sum + this.estimateTokens(m.content), 0);

      log.debug(`Loaded ${personaMemories.length} memories in ${loadTimeMs.toFixed(1)}ms (~${tokenCount} tokens)`);

      return {
        sourceName: this.name,
        tokenCount,
        loadTimeMs,
        memories: personaMemories,
        metadata: {
          memoryCount: personaMemories.length,
          coreCount: coreMemoryCount,
          semanticQuery: semanticQuery?.slice(0, 50)
        }
      };
    } catch (error: any) {
      log.error(`Failed to load memories: ${error.message}`);
      return this.emptySection(startTime, error.message);
    }
  }

  private buildSemanticQuery(context: RAGSourceContext): string | undefined {
    // Use currentMessage from options if available
    if (context.options.currentMessage?.content) {
      return context.options.currentMessage.content;
    }
    // Otherwise caller should populate via widgetContext or similar
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
