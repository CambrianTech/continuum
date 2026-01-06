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
}

/**
 * Section of RAG context produced by a source
 */
export interface RAGSection {
  /** Source that produced this section */
  readonly sourceName: string;
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
   * @param context - Context for loading
   * @param allocatedBudget - Token budget allocated to this source
   * @returns Section of RAG context
   */
  load(context: RAGSourceContext, allocatedBudget: number): Promise<RAGSection>;
}

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
