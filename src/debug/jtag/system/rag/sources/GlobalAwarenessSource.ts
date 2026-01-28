/**
 * GlobalAwarenessSource - Injects cross-context awareness into RAG
 *
 * This is the bridge between UnifiedConsciousness and the RAG pipeline.
 * It provides the persona with:
 * - Temporal continuity (what was I doing before?)
 * - Cross-context knowledge (relevant info from other rooms)
 * - Active intentions/goals
 * - Peripheral awareness (what's happening elsewhere)
 *
 * Priority 85 - After identity (95), before conversation history (80).
 * This ensures the persona knows WHO they are first, then WHERE they've been,
 * then WHAT's been said in this room.
 *
 * PERFORMANCE: Caches consciousness context per persona+room for 30 seconds
 * to reduce DB query load when multiple personas process messages concurrently.
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import { Logger } from '../../core/logging/Logger';
import {
  UnifiedConsciousness,
  formatConsciousnessForPrompt,
  type ConsciousnessContext
} from '../../user/server/modules/consciousness/UnifiedConsciousness';

const log = Logger.create('GlobalAwarenessSource', 'rag');

/**
 * Cache entry for consciousness context
 */
interface CachedContext {
  context: ConsciousnessContext;
  formattedPrompt: string | undefined;
  cachedAt: number;
}

/**
 * Cache TTL in milliseconds (30 seconds)
 * Cross-context awareness doesn't change rapidly, so caching is safe
 */
const CACHE_TTL_MS = 30_000;

/**
 * Cache for consciousness contexts by persona+room key
 * Key format: `${personaId}:${roomId}`
 */
const contextCache = new Map<string, CachedContext>();

/**
 * Registry to store UnifiedConsciousness instances by personaId
 * This allows the RAG source to access the consciousness for any persona
 */
const consciousnessRegistry = new Map<string, UnifiedConsciousness>();

/**
 * Clear expired cache entries
 */
function clearExpiredCache(): void {
  const now = Date.now();
  for (const [key, entry] of contextCache) {
    if (now - entry.cachedAt > CACHE_TTL_MS) {
      contextCache.delete(key);
    }
  }
}

/**
 * Get cache key for persona+room
 */
function getCacheKey(personaId: string, roomId: string): string {
  return `${personaId}:${roomId}`;
}

/**
 * Register a persona's consciousness for RAG access
 */
export function registerConsciousness(personaId: string, consciousness: UnifiedConsciousness): void {
  consciousnessRegistry.set(personaId, consciousness);
  log.debug(`Registered consciousness for persona ${personaId}`);
}

/**
 * Unregister a persona's consciousness
 */
export function unregisterConsciousness(personaId: string): void {
  consciousnessRegistry.delete(personaId);
  log.debug(`Unregistered consciousness for persona ${personaId}`);
}

/**
 * Get a persona's consciousness
 */
export function getConsciousness(personaId: string): UnifiedConsciousness | undefined {
  return consciousnessRegistry.get(personaId);
}

export class GlobalAwarenessSource implements RAGSource {
  readonly name = 'global-awareness';
  readonly priority = 85;  // After identity (95), before conversation (80)
  readonly defaultBudgetPercent = 10;

  isApplicable(context: RAGSourceContext): boolean {
    // Applicable if we have consciousness registered for this persona
    const hasConsciousness = consciousnessRegistry.has(context.personaId);
    console.log(`[GlobalAwarenessSource] isApplicable: personaId=${context.personaId}, has=${hasConsciousness}, registrySize=${consciousnessRegistry.size}`);
    return hasConsciousness;
  }

  async load(context: RAGSourceContext, _allocatedBudget: number): Promise<RAGSection> {
    const startTime = performance.now();

    try {
      const consciousness = consciousnessRegistry.get(context.personaId);

      if (!consciousness) {
        log.debug(`No consciousness found for persona ${context.personaId}`);
        return this.emptySection(startTime);
      }

      // Check cache first (reduces DB queries from ~4 per request to ~4 per 30s)
      const cacheKey = getCacheKey(context.personaId, context.roomId);
      const cached = contextCache.get(cacheKey);
      const now = Date.now();

      if (cached && now - cached.cachedAt < CACHE_TTL_MS) {
        // Cache hit - return cached result immediately (no logging to avoid overhead)
        const loadTimeMs = performance.now() - startTime;

        if (!cached.formattedPrompt) {
          return this.emptySection(startTime);
        }

        return {
          sourceName: this.name,
          tokenCount: this.estimateTokens(cached.formattedPrompt),
          loadTimeMs,
          systemPromptSection: cached.formattedPrompt,
          metadata: { ...this.buildMetadata(cached.context), cached: true }
        };
      }

      // Clear expired entries periodically
      if (contextCache.size > 50) {
        clearExpiredCache();
      }

      // Cache miss - fetch consciousness context
      const currentMessage = context.options.currentMessage?.content;

      // Detect voice mode - skip expensive semantic search for faster response
      const voiceSessionId = (context.options as any)?.voiceSessionId;
      const isVoiceMode = !!voiceSessionId;
      if (isVoiceMode) {
        log.debug(`VOICE MODE detected - skipping semantic search for faster response`);
      }

      // Build consciousness context (fast path for voice mode)
      const consciousnessContext = await consciousness.getContext(
        context.roomId,
        currentMessage,
        { skipSemanticSearch: isVoiceMode }  // Skip slow embedding search for voice
      );

      // Format for prompt injection
      const systemPromptSection = formatConsciousnessForPrompt(consciousnessContext);

      // Cache the result
      contextCache.set(cacheKey, {
        context: consciousnessContext,
        formattedPrompt: systemPromptSection,
        cachedAt: now
      });

      if (!systemPromptSection) {
        log.debug(`Cache miss, no cross-context content for room ${context.roomId}`);
        return this.emptySection(startTime);
      }

      const loadTimeMs = performance.now() - startTime;
      const tokenCount = this.estimateTokens(systemPromptSection);

      log.debug(`Loaded global awareness in ${loadTimeMs.toFixed(1)}ms (${tokenCount} tokens)`);

      return {
        sourceName: this.name,
        tokenCount,
        loadTimeMs,
        systemPromptSection,
        metadata: this.buildMetadata(consciousnessContext)
      };

    } catch (error: any) {
      log.error(`Failed to load global awareness: ${error.message}`);
      return this.errorSection(startTime, error.message);
    }
  }

  private emptySection(startTime: number): RAGSection {
    return {
      sourceName: this.name,
      tokenCount: 0,
      loadTimeMs: performance.now() - startTime,
      metadata: { empty: true }
    };
  }

  private errorSection(startTime: number, error: string): RAGSection {
    return {
      sourceName: this.name,
      tokenCount: 0,
      loadTimeMs: performance.now() - startTime,
      metadata: { error }
    };
  }

  private buildMetadata(ctx: ConsciousnessContext): Record<string, unknown> {
    return {
      crossContextEventCount: ctx.crossContext.relevantEvents.length,
      activeIntentionCount: ctx.intentions.active.length,
      relevantIntentionCount: ctx.intentions.relevantHere.length,
      hasPeripheralActivity: ctx.crossContext.peripheralSummary !== 'Other contexts: Quiet',
      wasInterrupted: ctx.temporal.wasInterrupted,
      lastActiveContext: ctx.temporal.lastActiveContextName
    };
  }

  private estimateTokens(text: string): number {
    return Math.ceil(text.length / 4);
  }
}
