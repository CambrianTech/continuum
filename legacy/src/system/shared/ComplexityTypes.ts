/**
 * Complexity Types - Shared type definitions for Adaptive Complexity Routing
 *
 * This module provides the core type system for the Progressive Scoring system,
 * enabling intelligent model selection based on real-time complexity assessment.
 *
 * **Purpose**: Democratize AI by routing messages to appropriate model tiers:
 * - Start with cheap/free models (local Candle)
 * - Detect complexity indicators during generation
 * - Upgrade to capable models only when needed
 * - Cost proportional to actual cognitive load required
 *
 * **Integration**: Used by ProgressiveScorer, ComplexityAssessor, and PersonaUser
 *
 * @see PHASE2-PROGRESSIVE-SCORING-PLAN.md - Implementation blueprint
 * @see ADAPTIVE-COMPLEXITY-ROUTING.md - Full architecture vision
 */

/**
 * Complexity classification levels for message assessment
 *
 * Determines routing to appropriate model tier:
 * - straightforward → local-fast (qwen2.5:7b, free)
 * - moderate → local-capable (Llama-3.1-70B, free)
 * - nuanced → api-premium (Claude 3.5 Sonnet, $0.003/msg)
 */
export type ComplexityLevel = 'straightforward' | 'moderate' | 'nuanced';

/**
 * Model tier classifications for routing decisions
 *
 * Ordered by cost and capability:
 * 1. local-fast: M1+ hardware, 7B models (qwen2.5:7b) - FREE
 * 2. local-capable: M1 Pro+ hardware, 70B models (Llama-3.1-70B) - FREE
 * 3. api-cheap: External APIs (deepseek-chat, groq) - $0.0001-0.001/msg
 * 4. api-premium: Premium APIs (Claude, GPT-4) - $0.003-0.005/msg
 *
 * Progressive scoring may trigger upgrades within session:
 * local-fast → local-capable → api-cheap → api-premium
 */
export type ModelTier = 'local-fast' | 'local-capable' | 'api-cheap' | 'api-premium';

/**
 * Assessment result from complexity classifier
 *
 * Created by ComplexityAssessor (Phase 1) for initial routing
 * Updated by ProgressiveScorer (Phase 2) during generation
 */
export interface ComplexityAssessment {
  /** Complexity level (straightforward/moderate/nuanced) */
  level: ComplexityLevel;

  /** Human-readable indicators that influenced classification */
  indicators: string[];

  /** Confidence score (0.0-1.0) in the assessment */
  confidence: number;

  /**
   * Token offset where this assessment was made
   * undefined = initial assessment
   * number = reassessment during streaming at this token offset
   */
  reassessedAt?: number;
}

/**
 * Upgrade decision from progressive scoring
 *
 * Returned by ProgressiveScorer.analyze() during token-window analysis
 * Indicates whether to upgrade to more capable model tier
 */
export interface ScoringResult {
  /** Whether upgrade is recommended */
  shouldUpgrade: boolean;

  /** Human-readable reason for upgrade decision */
  reason?: string;

  /** Target complexity level if upgrading */
  newLevel?: ComplexityLevel;
}

/**
 * Upgrade indicator detected during streaming
 *
 * Internal type used by ProgressiveScorer to track complexity signals
 */
export interface UpgradeIndicator {
  /** Type of indicator detected */
  type: 'hedging' | 'self-correction' | 'multi-perspective' | 'uncertainty' | 'clarification';

  /** Matched pattern text */
  pattern: string;

  /** Token offset where detected */
  offset: number;

  /** Confidence in this indicator (0.0-1.0) */
  confidence: number;
}

/**
 * Extended context for AI generation with routing metadata
 *
 * Passed to AI providers via AIProviderDaemon.generate()
 * Includes full history of complexity assessments and routing decisions
 */
export interface ResponseContext {
  /** Complexity assessment history */
  complexity: {
    /** Initial assessment before generation */
    initial: ComplexityAssessment;

    /** Current assessment (may differ if reassessed) */
    current: ComplexityAssessment;

    /** History of all reassessments during streaming */
    reassessed: ComplexityAssessment[];

    /** All detected complexity indicators */
    indicators: string[];
  };

  /** Routing decisions and model selection */
  routing: {
    /** Current model tier */
    tier: ModelTier;

    /** Actual model ID being used */
    model: string;

    /** Why this model was selected */
    reason: string;

    /** Whether this response was upgraded mid-stream */
    upgraded: boolean;

    /** Previous model if upgraded */
    previousModel?: string;
  };

  /** Performance tracking metrics */
  performance: {
    /** Tokens consumed */
    tokensUsed: number;

    /** Response latency in milliseconds */
    latencyMs: number;

    /** API cost (0 for local models) */
    cost: number;
  };
}

/**
 * Configuration for ProgressiveScorer
 *
 * Tunable thresholds that determine upgrade behavior
 */
export interface ProgressiveScorerConfig {
  /** Tokens between reassessments (default: 200) */
  windowSize: number;

  /** Upgrade thresholds */
  thresholds: {
    /** Number of indicators that trigger upgrade (default: 3) */
    indicatorCount: number;

    /** Minimum confidence to trust assessment (default: 0.6) */
    confidence: number;

    /** Maximum tokens before forced decision (default: 1000) */
    tokenBudget: number;
  };
}

/**
 * Default configuration for ProgressiveScorer
 *
 * Tuned based on Phase 2 implementation plan:
 * - 200 token windows = ~800 characters = ~30 seconds of generation
 * - 3 indicators = enough signal without false positives
 * - 0.6 confidence = balanced between sensitivity and specificity
 * - 1000 token budget = fail-safe to prevent excessive cheap model usage
 */
export const DEFAULT_PROGRESSIVE_SCORER_CONFIG: ProgressiveScorerConfig = {
  windowSize: 200,
  thresholds: {
    indicatorCount: 3,
    confidence: 0.6,
    tokenBudget: 1000
  }
};

/**
 * Model routing map: Complexity level → Recommended model tiers
 *
 * From ADAPTIVE-COMPLEXITY-ROUTING.md lines 72-77
 * Ordered by preference (try first option, fallback to next)
 */
export const ROUTING_MAP: Record<ComplexityLevel, ModelTier[]> = {
  straightforward: ['local-fast', 'local-capable', 'api-cheap'],
  moderate: ['local-capable', 'api-cheap', 'api-premium'],
  nuanced: ['api-premium', 'api-cheap', 'local-capable']
};

/**
 * Helper: Get recommended model tiers for a complexity level
 *
 * @param level - Complexity level
 * @returns Array of model tiers in preference order
 */
export function getRecommendedTiers(level: ComplexityLevel): ModelTier[] {
  return ROUTING_MAP[level];
}

/**
 * Helper: Check if a tier upgrade is valid
 *
 * @param from - Current tier
 * @param to - Target tier
 * @returns True if upgrade follows valid progression
 */
export function isValidUpgrade(from: ModelTier, to: ModelTier): boolean {
  const tierOrder: ModelTier[] = ['local-fast', 'local-capable', 'api-cheap', 'api-premium'];
  const fromIndex = tierOrder.indexOf(from);
  const toIndex = tierOrder.indexOf(to);

  // Upgrade must be to a higher tier (higher index)
  return toIndex > fromIndex;
}

/**
 * Helper: Get next tier in upgrade path
 *
 * @param current - Current tier
 * @returns Next tier or null if already at highest
 */
export function getNextTier(current: ModelTier): ModelTier | null {
  switch (current) {
    case 'local-fast':
      return 'local-capable';
    case 'local-capable':
      return 'api-cheap';
    case 'api-cheap':
      return 'api-premium';
    case 'api-premium':
      return null; // Already at highest tier
  }
}
