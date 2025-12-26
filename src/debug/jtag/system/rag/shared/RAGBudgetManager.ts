/**
 * RAGBudgetManager - Flexbox-like Token Budget Allocation
 *
 * Implements a CSS flexbox-inspired algorithm for allocating tokens
 * between multiple RAG sources (conversation history, memories, artifacts, etc.)
 *
 * Key concepts (borrowed from CSS Flexbox):
 * - flex-basis: Minimum token allocation (must have at least this)
 * - flex-grow: Priority weight for distributing extra tokens
 * - flex-shrink: How much to reduce when over budget
 * - max-width: Maximum tokens this source can use
 *
 * Usage:
 *   const manager = new RAGBudgetManager('gpt-4');
 *   const allocation = manager.allocate([
 *     { sourceId: 'conversation', priority: 10, minTokens: 1000, maxTokens: 5000 },
 *     { sourceId: 'memories', priority: 8, minTokens: 500, maxTokens: 2000 },
 *     { sourceId: 'artifacts', priority: 3, minTokens: 0, maxTokens: 1000 }
 *   ], { system: 500, completion: 3000 });
 */

import { getContextWindow, getRecommendedMaxOutputTokens } from '../../shared/ModelContextWindows';

/**
 * RAG source budget specification
 */
export interface RAGSourceBudget {
  /** Unique identifier for this source (e.g., 'conversation', 'memories') */
  readonly sourceId: string;

  /** Priority weight 1-10, higher = more important */
  readonly priority: number;

  /** Minimum tokens this source needs (flex-basis) */
  readonly minTokens: number;

  /** Preferred tokens if available */
  readonly preferredTokens?: number;

  /** Maximum tokens this source can use (flex-max) */
  readonly maxTokens: number;

  /** Whether this source is required (allocation fails if can't meet minTokens) */
  readonly required?: boolean;
}

/**
 * Token allocation result for a single source
 */
export interface SourceAllocation {
  readonly sourceId: string;
  readonly allocatedTokens: number;
  readonly requestedMin: number;
  readonly requestedMax: number;
  readonly utilizationPercent: number;  // allocated / max
}

/**
 * Full budget allocation result
 */
export interface BudgetAllocation {
  readonly modelId: string;
  readonly contextWindow: number;
  readonly reservedTokens: {
    readonly system: number;
    readonly completion: number;
  };
  readonly availableForSources: number;
  readonly allocations: SourceAllocation[];
  readonly totalAllocated: number;
  readonly unallocatedTokens: number;
  readonly warnings: string[];
}

/**
 * Reserved token specification
 */
export interface ReservedTokens {
  /** System prompt tokens */
  system: number;

  /** Completion/output tokens */
  completion: number;
}

/**
 * RAGBudgetManager - Flexbox-style token allocation
 */
export class RAGBudgetManager {
  private readonly modelId: string;
  private readonly contextWindow: number;

  constructor(modelId: string) {
    this.modelId = modelId;
    this.contextWindow = getContextWindow(modelId);
  }

  /**
   * Allocate tokens to RAG sources using flexbox algorithm
   *
   * Algorithm:
   * 1. Reserve tokens for system prompt and completion
   * 2. Allocate minimum tokens to all sources (flex-basis)
   * 3. Distribute remaining tokens by priority weight (flex-grow)
   * 4. Cap each source at its maximum (flex-max)
   * 5. Redistribute any capped overflow to other sources
   *
   * @param sources - RAG source budget specifications
   * @param reserved - Reserved tokens for system/completion
   * @returns Budget allocation result
   */
  allocate(sources: RAGSourceBudget[], reserved: ReservedTokens): BudgetAllocation {
    const warnings: string[] = [];

    // 1. Calculate available tokens
    const totalReserved = reserved.system + reserved.completion;
    const availableForSources = this.contextWindow - totalReserved;

    if (availableForSources <= 0) {
      warnings.push(`No tokens available after reservations (${totalReserved} reserved > ${this.contextWindow} context)`);
      return this.createEmptyAllocation(sources, reserved, warnings);
    }

    // 2. Sort sources by priority (highest first)
    const sortedSources = [...sources].sort((a, b) => b.priority - a.priority);

    // 3. Allocate minimums first (flex-basis)
    const totalMinRequired = sortedSources.reduce((sum, s) => sum + s.minTokens, 0);

    if (totalMinRequired > availableForSources) {
      // Can't satisfy minimums - allocate proportionally
      warnings.push(`Minimum requirements (${totalMinRequired}) exceed available (${availableForSources})`);
      return this.allocateProportionally(sortedSources, availableForSources, reserved, warnings);
    }

    // 4. Allocate minimums and track remaining
    let remainingTokens = availableForSources;
    const allocations: Map<string, number> = new Map();

    for (const source of sortedSources) {
      allocations.set(source.sourceId, source.minTokens);
      remainingTokens -= source.minTokens;
    }

    // 5. Distribute remaining tokens by priority weight (flex-grow)
    const totalPriority = sortedSources.reduce((sum, s) => sum + s.priority, 0);

    while (remainingTokens > 0) {
      let tokensDistributed = 0;

      for (const source of sortedSources) {
        const current = allocations.get(source.sourceId) || 0;

        // Skip if already at max
        if (current >= source.maxTokens) {
          continue;
        }

        // Calculate share based on priority
        const priorityShare = source.priority / totalPriority;
        const share = Math.floor(remainingTokens * priorityShare);

        // Cap at max
        const actualGain = Math.min(share, source.maxTokens - current);

        if (actualGain > 0) {
          allocations.set(source.sourceId, current + actualGain);
          tokensDistributed += actualGain;
        }
      }

      remainingTokens -= tokensDistributed;

      // If no tokens distributed, all sources are at max
      if (tokensDistributed === 0) {
        break;
      }
    }

    // 6. Build result
    const result: SourceAllocation[] = sortedSources.map(source => {
      const allocated = allocations.get(source.sourceId) || 0;
      return {
        sourceId: source.sourceId,
        allocatedTokens: allocated,
        requestedMin: source.minTokens,
        requestedMax: source.maxTokens,
        utilizationPercent: source.maxTokens > 0 ? (allocated / source.maxTokens) * 100 : 0
      };
    });

    const totalAllocated = result.reduce((sum, r) => sum + r.allocatedTokens, 0);

    return {
      modelId: this.modelId,
      contextWindow: this.contextWindow,
      reservedTokens: reserved,
      availableForSources,
      allocations: result,
      totalAllocated,
      unallocatedTokens: remainingTokens,
      warnings
    };
  }

  /**
   * Allocate proportionally when minimums can't be satisfied
   */
  private allocateProportionally(
    sources: RAGSourceBudget[],
    available: number,
    reserved: ReservedTokens,
    warnings: string[]
  ): BudgetAllocation {
    const totalMin = sources.reduce((sum, s) => sum + s.minTokens, 0);
    const ratio = available / totalMin;

    const allocations: SourceAllocation[] = sources.map(source => {
      const allocated = Math.floor(source.minTokens * ratio);
      return {
        sourceId: source.sourceId,
        allocatedTokens: allocated,
        requestedMin: source.minTokens,
        requestedMax: source.maxTokens,
        utilizationPercent: source.maxTokens > 0 ? (allocated / source.maxTokens) * 100 : 0
      };
    });

    const totalAllocated = allocations.reduce((sum, r) => sum + r.allocatedTokens, 0);

    return {
      modelId: this.modelId,
      contextWindow: this.contextWindow,
      reservedTokens: reserved,
      availableForSources: available,
      allocations,
      totalAllocated,
      unallocatedTokens: available - totalAllocated,
      warnings
    };
  }

  /**
   * Create empty allocation result
   */
  private createEmptyAllocation(
    sources: RAGSourceBudget[],
    reserved: ReservedTokens,
    warnings: string[]
  ): BudgetAllocation {
    const allocations: SourceAllocation[] = sources.map(source => ({
      sourceId: source.sourceId,
      allocatedTokens: 0,
      requestedMin: source.minTokens,
      requestedMax: source.maxTokens,
      utilizationPercent: 0
    }));

    return {
      modelId: this.modelId,
      contextWindow: this.contextWindow,
      reservedTokens: reserved,
      availableForSources: 0,
      allocations,
      totalAllocated: 0,
      unallocatedTokens: 0,
      warnings
    };
  }

  /**
   * Get recommended budget for chat RAG
   * Pre-configured budgets for common sources
   */
  static getChatBudget(modelId: string): RAGSourceBudget[] {
    const contextWindow = getContextWindow(modelId);
    const isLargeContext = contextWindow > 32768;

    return [
      {
        sourceId: 'conversation',
        priority: 10,  // Highest - recent context is most important
        minTokens: isLargeContext ? 2000 : 1000,
        preferredTokens: isLargeContext ? 8000 : 3000,
        maxTokens: isLargeContext ? 20000 : 6000,
        required: true
      },
      {
        sourceId: 'memories',
        priority: 7,  // High - semantic recall of relevant past
        minTokens: 500,
        preferredTokens: isLargeContext ? 3000 : 1500,
        maxTokens: isLargeContext ? 8000 : 3000,
        required: false
      },
      {
        sourceId: 'artifacts',
        priority: 5,  // Medium - supporting images/files
        minTokens: 0,
        preferredTokens: isLargeContext ? 2000 : 1000,
        maxTokens: isLargeContext ? 4000 : 2000,
        required: false
      },
      {
        sourceId: 'tools',
        priority: 4,  // Medium-low - tool descriptions
        minTokens: 0,
        preferredTokens: 500,
        maxTokens: 1500,
        required: false
      }
    ];
  }

  /**
   * Get recommended reserved tokens for a model
   */
  static getRecommendedReserved(modelId: string): ReservedTokens {
    return {
      system: 500,  // System prompt estimate
      completion: getRecommendedMaxOutputTokens(modelId)
    };
  }
}

/**
 * Quick allocation for chat context
 * Convenience function that uses default chat budgets
 */
export function allocateChatBudget(modelId: string): BudgetAllocation {
  const manager = new RAGBudgetManager(modelId);
  const sources = RAGBudgetManager.getChatBudget(modelId);
  const reserved = RAGBudgetManager.getRecommendedReserved(modelId);
  return manager.allocate(sources, reserved);
}
