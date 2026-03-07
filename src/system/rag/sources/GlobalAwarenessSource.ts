/**
 * GlobalAwarenessSource - Injects cross-context awareness into RAG via Rust IPC
 *
 * Delegates to Rust's consciousness context builder which runs:
 * - Temporal continuity queries (what was I doing before?)
 * - Cross-context event aggregation (what happened in other rooms?)
 * - Active intention detection (interrupted tasks)
 * - Peripheral activity check
 *
 * All queries run concurrently in Rust with separate SQLite read connections,
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
 *
 * Priority 85 - After identity (95), before conversation history (80).
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import type { RagSourceRequest, RagSourceResult } from '../../../shared/generated/rag';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('GlobalAwarenessSource', 'rag');

/**
 * Registry for consciousness instances — kept for backward compatibility.
 * The actual consciousness context is now built in Rust via IPC,
 * but we still need the registry to check if a persona has been initialized.
 */
const initializedPersonas = new Set<string>();

/**
 * Register a persona as having consciousness initialized.
 * Called during PersonaUser startup after memory/init succeeds.
 */
export function registerConsciousness(personaId: string, _consciousness?: any): void {
  initializedPersonas.add(personaId);
  log.debug(`Registered consciousness for persona ${personaId}`);
}

/**
 * Unregister a persona's consciousness
 */
export function unregisterConsciousness(personaId: string): void {
  initializedPersonas.delete(personaId);
  log.debug(`Unregistered consciousness for persona ${personaId}`);
}

/**
 * Check if a persona has consciousness registered
 */
export function getConsciousness(personaId: string): boolean {
  return initializedPersonas.has(personaId);
}

export class GlobalAwarenessSource implements RAGSource {
  readonly name = 'global-awareness';
  readonly priority = 85;  // After identity (95), before conversation (80)
  readonly defaultBudgetPercent = 5;
  readonly supportsBatching = true;  // Participate in batched Rust IPC

  // Negative cache with exponential backoff: skip IPC when corpus unavailable.
  // Backoff: 2s → 4s → 8s → 16s → 30s (cap). Resets on success.
  private static _corpusBackoff: Map<string, { failedAt: number; ttlMs: number }> = new Map();
  private static readonly BACKOFF_MIN_MS = 2_000;
  private static readonly BACKOFF_MAX_MS = 30_000;

  isApplicable(context: RAGSourceContext): boolean {
    if (!initializedPersonas.has(context.personaId)) return false;

    const backoff = GlobalAwarenessSource._corpusBackoff.get(context.personaId);
    if (backoff && (Date.now() - backoff.failedAt) < backoff.ttlMs) {
      return false;
    }
    return true;
  }

  /**
   * Get the batch request for this source.
   * Returns a RagSourceRequest with source_type='consciousness' for Rust rag/compose.
   */
  getBatchRequest(context: RAGSourceContext, allocatedBudget: number): RagSourceRequest | null {
    // Detect voice mode — skip expensive semantic search for faster response
    const voiceSessionId = (context.options as any)?.voiceSessionId;
    const isVoiceMode = !!voiceSessionId;

    const currentMessage = context.options.currentMessage?.content;

    return {
      source_type: 'consciousness',
      budget_tokens: allocatedBudget,
      params: {
        current_message: currentMessage,
        skip_semantic_search: isVoiceMode,
      }
    };
  }

  /**
   * Convert Rust RagSourceResult to TypeScript RAGSection.
   * Maps consciousness result to systemPromptSection.
   */
  fromBatchResult(result: RagSourceResult, loadTimeMs: number): RAGSection {
    // Consciousness result has formatted_prompt in the first section
    const formattedPrompt = result.sections
      .map(s => s.content)
      .filter(Boolean)
      .join('\n\n');

    if (!formattedPrompt) {
      return this.createEmptySection(loadTimeMs);
    }

    // Extract metadata from the result
    const metadata = result.metadata as any || {};

    return {
      sourceName: this.name,
      tokenCount: result.tokens_used,
      loadTimeMs,
      systemPromptSection: formattedPrompt,
      metadata: {
        crossContextEventCount: metadata.cross_context_event_count,
        activeIntentionCount: metadata.active_intention_count,
        hasPeripheralActivity: metadata.has_peripheral_activity,
        wasInterrupted: metadata.temporal?.was_interrupted,
        lastActiveContext: metadata.temporal?.last_active_context_name,
        rustBuildMs: metadata.build_time_ms
      }
    };
  }

  /**
   * Load data via individual IPC call (fallback when batching not used).
   * Note: When batching is enabled, this method is typically not called.
   * RAGComposer uses getBatchRequest() + fromBatchResult() instead.
   */
  async load(context: RAGSourceContext, _allocatedBudget: number): Promise<RAGSection> {
    const startTime = performance.now();

    try {
      // Get PersonaUser to access Rust bridge
      const { UserDaemonServer } = await import('../../../daemons/user-daemon/server/UserDaemonServer');
      const userDaemon = UserDaemonServer.getInstance();

      if (!userDaemon) {
        log.debug('UserDaemon not available, skipping awareness');
        return this.createEmptySection(performance.now() - startTime);
      }

      const personaUser = userDaemon.getPersonaUser(context.personaId);
      if (!personaUser) {
        return this.createEmptySection(performance.now() - startTime);
      }

      // Access the Rust cognition bridge (nullable getter — no throw)
      const bridge = (personaUser as any).rustCognitionBridge;
      if (!bridge) {
        log.debug('Rust cognition bridge not available, skipping awareness');
        return this.createEmptySection(performance.now() - startTime);
      }

      // Detect voice mode — skip expensive semantic search for faster response
      const voiceSessionId = (context.options as any)?.voiceSessionId;
      const isVoiceMode = !!voiceSessionId;

      const currentMessage = context.options.currentMessage?.content;

      // Single IPC call → Rust builds consciousness context with concurrent SQLite reads
      // Rust handles its own 30s TTL cache internally
      const result = await bridge.memoryConsciousnessContext(
        context.roomId,
        currentMessage,
        isVoiceMode  // skipSemanticSearch
      );

      // Clear backoff on success — corpus is available
      GlobalAwarenessSource._corpusBackoff.delete(context.personaId);

      if (!result.formatted_prompt) {
        log.debug(`No cross-context content for room ${context.roomId}`);
        return this.createEmptySection(performance.now() - startTime);
      }

      const loadTimeMs = performance.now() - startTime;
      const tokenCount = this.estimateTokens(result.formatted_prompt);

      log.debug(`Loaded global awareness in ${loadTimeMs.toFixed(1)}ms (${tokenCount} tokens) rust=${result.build_time_ms.toFixed(1)}ms`);

      return {
        sourceName: this.name,
        tokenCount,
        loadTimeMs,
        systemPromptSection: result.formatted_prompt,
        metadata: {
          crossContextEventCount: result.cross_context_event_count,
          activeIntentionCount: result.active_intention_count,
          hasPeripheralActivity: result.has_peripheral_activity,
          wasInterrupted: result.temporal.was_interrupted,
          lastActiveContext: result.temporal.last_active_context_name,
          rustBuildMs: result.build_time_ms
        }
      };

    } catch (error: any) {
      // Negative-cache "No memory corpus" errors with exponential backoff
      if (error.message?.includes('No memory corpus')) {
        const prev = GlobalAwarenessSource._corpusBackoff.get(context.personaId);
        const nextTtl = Math.min(
          (prev?.ttlMs ?? GlobalAwarenessSource.BACKOFF_MIN_MS) * 2,
          GlobalAwarenessSource.BACKOFF_MAX_MS
        );
        GlobalAwarenessSource._corpusBackoff.set(context.personaId, { failedAt: Date.now(), ttlMs: nextTtl });
        log.debug(`Corpus unavailable for ${context.personaId.slice(0, 8)}, backoff ${nextTtl}ms`);
      } else {
        log.error(`Failed to load global awareness: ${error.message}`);
      }
      return this.createErrorSection(performance.now() - startTime, error.message);
    }
  }

  private createEmptySection(loadTimeMs: number): RAGSection {
    return {
      sourceName: this.name,
      tokenCount: 0,
      loadTimeMs,
      metadata: { empty: true }
    };
  }

  private createErrorSection(loadTimeMs: number, error: string): RAGSection {
    return {
      sourceName: this.name,
      tokenCount: 0,
      loadTimeMs,
      metadata: { error }
    };
  }

  private estimateTokens(text: string): number {
    return Math.ceil(text.length / 4);
  }
}
