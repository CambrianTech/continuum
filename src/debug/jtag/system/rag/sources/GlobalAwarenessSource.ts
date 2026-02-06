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
  readonly defaultBudgetPercent = 10;
  readonly supportsBatching = true;  // Participate in batched Rust IPC

  // Negative cache: when Rust returns "No memory corpus", skip IPC for 60s.
  // Without this, each failing persona makes a 1-3s IPC call every RAG build
  // that returns nothing but an error — pure waste.
  private static _corpusUnavailable: Map<string, number> = new Map();
  private static readonly NEGATIVE_CACHE_TTL_MS = 60_000;

  isApplicable(context: RAGSourceContext): boolean {
    if (!initializedPersonas.has(context.personaId)) return false;

    // Skip if we recently learned this persona's corpus is unavailable
    const failedAt = GlobalAwarenessSource._corpusUnavailable.get(context.personaId);
    if (failedAt && (Date.now() - failedAt) < GlobalAwarenessSource.NEGATIVE_CACHE_TTL_MS) {
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
      // Negative-cache "No memory corpus" errors — skip IPC for 60s
      if (error.message?.includes('No memory corpus')) {
        GlobalAwarenessSource._corpusUnavailable.set(context.personaId, Date.now());
        log.debug(`Corpus unavailable for ${context.personaId.slice(0, 8)}, negative-cached for 60s`);
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
