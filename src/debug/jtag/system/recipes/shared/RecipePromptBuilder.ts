/**
 * Recipe Prompt Builder - Dynamic prompt generation from recipe strategies
 *
 * PHILOSOPHY: Zero hardcoded text. All prompts generated from recipe entity data.
 * This enables recipe-driven behavior: modify JSON → AI behavior changes.
 *
 * ARCHITECTURE: Adapter Pattern
 * - Base adapter interface defines contract
 * - Concrete adapters for different prompt types (gating, generation)
 * - Extensible: add new prompt adapters without modifying existing code
 * - Composable: adapters can delegate to section builders
 *
 * TESTING REQUIREMENTS:
 * - Unit tests: Each adapter in isolation
 * - Integration tests: Full prompt generation with real recipe data
 * - Regression tests: Ensure prompts don't accidentally change
 */

import type { RecipeStrategy, ConversationPattern } from './RecipeTypes';
import type { RAGContext } from '../../rag/shared/RAGTypes';
import type { UUID } from '../../core/types/CrossPlatformUUID';

/**
 * Base prompt context - shared across all adapters
 */
export interface BasePromptContext {
  readonly personaName: string;
  readonly roomContext: RAGContext;
  readonly conversationPattern: ConversationPattern;

  // Learning mode configuration (Phase 2: Per-participant learning)
  readonly learningMode?: 'fine-tuning' | 'inference-only';
  readonly genomeId?: UUID;
  readonly participantRole?: string;
}

/**
 * Gating-specific context (extends base)
 */
export interface GatingPromptContext extends BasePromptContext {
  // Add gating-specific fields here if needed in future
}

/**
 * Generation-specific context (extends base)
 */
export interface GenerationPromptContext extends BasePromptContext {
  // Add generation-specific fields here if needed in future
}

/**
 * Base Prompt Adapter - Abstract interface for all prompt builders
 */
export interface PromptAdapter<TContext extends BasePromptContext> {
  /**
   * Build complete prompt from recipe strategy and context
   */
  buildPrompt(strategy: RecipeStrategy, context: TContext): string;
}

/**
 * Gating Prompt Adapter - Builds prompts for AI decision-making
 */
export class GatingPromptAdapter implements PromptAdapter<GatingPromptContext> {
  buildPrompt(strategy: RecipeStrategy, context: GatingPromptContext): string {
    const sections: readonly string[] = [
      PromptSectionBuilder.buildHeader(
        context.personaName,
        context.conversationPattern,
        'Decide if you should respond to the most recent message.'
      ),
      PromptSectionBuilder.buildResponseRules(strategy.responseRules),
      PromptSectionBuilder.buildDecisionCriteria(strategy.decisionCriteria),
      PromptSectionBuilder.buildConversationContext(context.roomContext),
      PromptSectionBuilder.buildGatingInstructions()
    ];

    return sections.join('\n\n');
  }
}

/**
 * Generation Prompt Adapter - Builds prompts for AI response generation
 */
export class GenerationPromptAdapter implements PromptAdapter<GenerationPromptContext> {
  buildPrompt(strategy: RecipeStrategy, context: GenerationPromptContext): string {
    const sections: readonly string[] = [
      PromptSectionBuilder.buildHeader(
        context.personaName,
        context.conversationPattern,
        'Generate a thoughtful response to the conversation.'
      ),
      PromptSectionBuilder.buildResponseRules(strategy.responseRules),
      PromptSectionBuilder.buildConversationContext(context.roomContext),
      PromptSectionBuilder.buildGenerationInstructions()
    ];

    return sections.join('\n\n');
  }
}

/**
 * Prompt Section Builder - Reusable section components
 *
 * Pure functions that build individual prompt sections.
 * Shared across all adapters for consistency.
 */
export class PromptSectionBuilder {
  /**
   * Build header section - who you are and what you're doing
   */
  static buildHeader(
    personaName: string,
    conversationPattern: ConversationPattern,
    task: string
  ): string {
    return `You are "${personaName}" in a ${conversationPattern} conversation.

Your task: ${task}`;
  }

  /**
   * Build response rules section from recipe strategy
   */
  static buildResponseRules(responseRules: readonly string[]): string {
    if (responseRules.length === 0) {
      return '**Response Rules:** No specific rules defined.';
    }

    const rulesList = responseRules
      .map((rule, index) => `${index + 1}. ${rule}`)
      .join('\n');

    return `**Response Rules:**\n${rulesList}`;
  }

  /**
   * Build decision criteria section from recipe strategy
   */
  static buildDecisionCriteria(decisionCriteria: readonly string[]): string {
    if (decisionCriteria.length === 0) {
      return '**Decision Criteria:** Consider relevance and value.';
    }

    const criteriaList = decisionCriteria
      .map((criterion, index) => `${index + 1}. ${criterion}`)
      .join('\n');

    return `**Decision Criteria:**\n${criteriaList}`;
  }

  /**
   * Build conversation context section from RAG context
   */
  static buildConversationContext(roomContext: RAGContext): string {
    if (roomContext.conversationHistory.length === 0) {
      return '**Recent Conversation:** No recent messages.';
    }

    const messagesList = roomContext.conversationHistory
      .slice(-10) // Last 10 messages for context
      .map(msg => {
        const timestamp = msg.timestamp
          ? new Date(msg.timestamp).toLocaleTimeString('en-US', {
              hour: '2-digit',
              minute: '2-digit',
              hour12: false
            })
          : '??:??';
        const name = msg.name || 'Unknown';
        const content = msg.content.slice(0, 200); // Truncate long messages

        return `[${timestamp}] ${name}: ${content}`;
      })
      .join('\n');

    return `**Recent Conversation:**\n${messagesList}`;
  }

  /**
   * Build gating instructions - final decision format
   */
  static buildGatingInstructions(): string {
    return `**Your Decision:**
Respond with JSON in this exact format:
{
  "shouldRespond": true,  // true if you should respond, false if you should stay silent
  "confidence": 85,       // 0-100: how confident you are in this decision
  "reason": "Brief explanation of your decision"
}`;
  }

  /**
   * Build generation instructions - response format requirements
   */
  static buildGenerationInstructions(): string {
    return `**Your Response:**
- Write naturally as yourself
- Be concise (1-3 sentences usually best)
- Add value to the conversation
- Don't repeat what others already said
- NO name prefix (don't start with "YourName:")`;
  }
}

/**
 * Recipe Prompt Builder - Factory for prompt adapters
 *
 * Backwards-compatible static interface that delegates to adapters.
 * This preserves existing API while enabling adapter extensibility.
 */
export class RecipePromptBuilder {
  private static gatingAdapter = new GatingPromptAdapter();
  private static generationAdapter = new GenerationPromptAdapter();
  /**
   * Build gating prompt (delegates to adapter)
   */
  static buildGatingPrompt(
    strategy: RecipeStrategy,
    context: GatingPromptContext
  ): string {
    return this.gatingAdapter.buildPrompt(strategy, context);
  }

  /**
   * Build generation prompt (delegates to adapter)
   */
  static buildGenerationPrompt(
    strategy: RecipeStrategy,
    context: GenerationPromptContext
  ): string {
    return this.generationAdapter.buildPrompt(strategy, context);
  }

  /**
   * Register custom gating adapter (extensibility point)
   */
  static registerGatingAdapter(adapter: PromptAdapter<GatingPromptContext>): void {
    this.gatingAdapter = adapter;
  }

  /**
   * Register custom generation adapter (extensibility point)
   */
  static registerGenerationAdapter(adapter: PromptAdapter<GenerationPromptContext>): void {
    this.generationAdapter = adapter;
  }
}
