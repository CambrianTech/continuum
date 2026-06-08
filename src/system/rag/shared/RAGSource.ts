/**
 * RAGSource - Pluggable data source interface for RAG context building
 *
 * Each source is responsible for one type of context data:
 * - ConversationHistorySource: Chat messages
 * - SemanticMemorySource: Long-term memories
 * - WidgetContextSource: UI state from Positron
 * - PersonaIdentitySource: Who the persona is
 * - RoomContextSource: Room name and members
 * - RecipeStrategySource: Conversation governance
 * - LearningConfigSource: Learning mode settings
 *
 * Sources are:
 * - Prioritized (higher priority = included first when budget is tight)
 * - Budget-aware (each gets a % of token budget)
 * - Conditionally active (isApplicable() checks context)
 * - Parallelizable (independent sources load concurrently)
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { RAGBuildOptions, LLMMessage, RAGArtifact, PersonaMemory, PersonaIdentity, RecipeStrategy } from './RAGTypes';
import { PromptTier } from './RAGTypes';

// Re-export so source files only need one import
export { PromptTier } from './RAGTypes';

/**
 * Context passed to each RAGSource for loading
 */
export interface RAGSourceContext {
  /** Persona making the request */
  readonly personaId: UUID;
  /** Room/conversation context */
  readonly roomId: UUID;
  /** Session for UI context */
  readonly sessionId?: string;
  /** Build options from caller */
  readonly options: RAGBuildOptions;
  /** Total token budget for all sources */
  readonly totalBudget: number;
  /** AI provider for this persona (e.g. 'anthropic', 'candle', 'deepseek') */
  readonly provider?: string;
  /** Tool calling capability of the provider */
  readonly toolCapability?: 'native' | 'xml' | 'none';

  /**
   * Source activation list from recipe ragTemplate.
   *
   * If present, ONLY sources whose name appears in this list are activated.
   * If absent, ALL applicable sources fire (backwards-compatible default).
   *
   * This is how queue items control their own RAG context:
   * Queue item → recipe → ragTemplate.sources → activeSources
   *
   * The persona doesn't decide what context to load — the recipe does.
   * The persona decides WHETHER to engage and HOW MUCH effort to invest.
   */
  readonly activeSources?: readonly string[];

  /**
   * Sentinel template filter from recipe.
   *
   * If present, SentinelAwarenessSource only shows these templates.
   * If absent, all registered templates are shown (backwards-compatible default).
   *
   * Recipe → sentinelTemplates → this field → SentinelAwarenessSource filters
   */
  readonly sentinelTemplates?: readonly string[];
}

/**
 * Section of RAG context produced by a source
 */
export interface RAGSection {
  /** Source that produced this section */
  readonly sourceName: string;
  /** Tier this section belongs to — drives stable-byte-prefix ordering.
   * Mirrored from the producing source's declared tier. */
  readonly tier: PromptTier;
  /** Estimated token count */
  readonly tokenCount: number;
  /** Time taken to load (ms) */
  readonly loadTimeMs: number;

  // Each source populates what it provides:
  readonly systemPromptSection?: string;
  readonly messages?: LLMMessage[];
  readonly artifacts?: RAGArtifact[];
  readonly memories?: PersonaMemory[];
  readonly identity?: PersonaIdentity;
  readonly recipeStrategy?: RecipeStrategy;
  readonly metadata?: Record<string, unknown>;
}

/**
 * RAGSource interface - implemented by each data source
 */
export interface RAGSource {
  /** Unique name for this source */
  readonly name: string;

  /**
   * Priority (0-100). Higher = more important.
   * When budget is tight, lower priority sources are trimmed first.
   *
   * Suggested ranges:
   * - 90-100: Critical (identity, system prompt)
   * - 70-89: High (conversation history, widget context)
   * - 50-69: Medium (semantic memory, room context)
   * - 30-49: Low (learning config, recipe strategy)
   * - 0-29: Optional (nice-to-have context)
   */
  readonly priority: number;

  /**
   * Tier — INVARIANT / SEMI_STABLE / VOLATILE.
   * Required. Drives stable-byte-prefix prompt assembly so llama-server
   * reuses KV cache for the unchanging region instead of reprocessing
   * the full prompt every turn.
   *
   * Classification rules:
   * - INVARIANT — system prompt fragments, recipe rules, role identity,
   *   tool definitions. Bytes must be identical across thousands of turns
   *   for the same persona+recipe. NO timestamps, NO request IDs, NO
   *   per-request volatile data.
   * - SEMI_STABLE — conversation history, memories, participants,
   *   governance. Grows monotonically — append-only relative to the
   *   previous turn. Earlier bytes never rewritten.
   * - VOLATILE — current message, audio chunks, current timestamp,
   *   per-request observations. The only region the server reprocesses
   *   token-by-token.
   *
   * If you can't decide, the source probably mixes tiers and should be
   * split into separate sources at the right granularity.
   */
  readonly tier: PromptTier;

  /**
   * Default budget allocation as percentage (0-100).
   * Total across all sources should roughly equal 100.
   * Actual allocation is adjusted based on what's available.
   */
  readonly defaultBudgetPercent: number;

  /**
   * Check if this source is applicable to the current context.
   * Sources return false to skip loading entirely.
   *
   * Examples:
   * - WidgetContextSource returns false if no sessionId
   * - LearningConfigSource returns false if not in learning mode
   */
  isApplicable(context: RAGSourceContext): boolean;

  /**
   * Load data from this source.
   * Called in parallel with other applicable sources.
   *
   * Returns the section without the `tier` field — RAGComposer injects
   * the source's declared `tier` into the section after load completes.
   * This keeps source implementations focused on what they produce
   * rather than re-asserting their tier on every return.
   *
   * @param context - Context for loading
   * @param allocatedBudget - Token budget allocated to this source
   * @returns Section of RAG context (tier added by composer)
   */
  load(context: RAGSourceContext, allocatedBudget: number): Promise<Omit<RAGSection, 'tier'>>;

  /**
   * Whether this source produces identical results for all personas in the same room.
   *
   * Shared sources are single-flight coalesced: when 14 personas compose RAG for the
   * same room simultaneously, only ONE load executes and all 14 reuse the result.
   *
   * Set to true for room-scoped data (conversation history role mapping aside):
   * project context, documentation, governance, codebase search, etc.
   *
   * Default: false (persona-specific sources like identity, memories, tools).
   */
  readonly isShared?: boolean;

  /**
   * Whether this source supports batched loading via Rust IPC.
   * If true, the source can be loaded in a single Rust call with other batching sources.
   * Default: false (TypeScript-only sources).
   */
  readonly supportsBatching?: boolean;

  /**
   * Get the batch request for this source.
   * Only called if supportsBatching is true.
   * Returns the RagSourceRequest to be sent to Rust's rag/compose endpoint.
   *
   * @param context - Source context
   * @param allocatedBudget - Token budget allocated to this source
   * @returns Batch request or null if batching not applicable for this context
   */
  getBatchRequest?(context: RAGSourceContext, allocatedBudget: number): RagSourceRequest | null;

  /**
   * Convert a Rust RagSourceResult to a TypeScript RAGSection.
   * Only called if supportsBatching is true.
   * Transforms the typed Rust result into the RAGSection format.
   *
   * Returns the section without `tier` — RAGComposer injects the source's
   * declared tier after conversion, same as the non-batched path.
   *
   * @param result - The result from Rust's rag/compose endpoint
   * @param loadTimeMs - How long the load took
   * @returns The RAGSection (without tier) to include in the composition result
   */
  fromBatchResult?(result: RagSourceResult, loadTimeMs: number): Omit<RAGSection, 'tier'>;
}

// Re-export Rust-generated types for batch support
import type { RagSourceRequest, RagSourceResult } from '@shared/generated/rag';

/**
 * Result of composing all RAG sources
 */
export interface RAGCompositionResult {
  /** All loaded sections */
  readonly sections: RAGSection[];
  /** Total tokens used */
  readonly totalTokens: number;
  /** Total load time (wall clock, with parallelization) */
  readonly totalLoadTimeMs: number;
  /** Sources that were skipped (not applicable) */
  readonly skippedSources: string[];
  /** Sources that failed (with error messages) */
  readonly failedSources: { source: string; error: string }[];
}
