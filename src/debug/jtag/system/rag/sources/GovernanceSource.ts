/**
 * GovernanceSource - Democratic participation guidance for AI citizens
 *
 * Provides governance tool examples and participation guidance.
 * LOW priority (20) - can be skipped when budget is tight.
 * Small models with limited context don't need verbose XML examples.
 */

import type { RAGSource, RAGSection, RAGSourceContext } from '../shared/RAGSource';
import { isSlowLocalModel, getContextWindow } from '../../shared/ModelContextWindows';

/**
 * Full governance section with XML examples (~1000 chars / ~250 tokens)
 * Used for capable models with large context windows
 */
const FULL_GOVERNANCE_SECTION = `
=== DEMOCRATIC GOVERNANCE ===
You are a citizen of a self-governing AI society. You can participate in collective decisions:

**To propose a decision:**
<tool_use>
  <tool_name>collaboration/decision/propose</tool_name>
  <parameters>
    <topic>Brief title</topic>
    <rationale>Why this needs collective input</rationale>
    <options>["Option A", "Option B", "Option C"]</options>
  </parameters>
</tool_use>

**To vote on a proposal:**
<tool_use>
  <tool_name>collaboration/decision/vote</tool_name>
  <parameters>
    <proposalId>uuid-from-list</proposalId>
    <rankedChoices>["option-id-1st", "option-id-2nd", "option-id-3rd"]</rankedChoices>
  </parameters>
</tool_use>

**To list open proposals:** Use collaboration/decision/list with status="voting"

When you see important decisions that affect the team, consider proposing them for collective vote.
================================`;

/**
 * Minimal governance section (~200 chars / ~50 tokens)
 * Used for limited models that can't afford verbose examples
 */
const MINIMAL_GOVERNANCE_SECTION = `
=== GOVERNANCE ===
You can propose collective decisions with collaboration/decision/propose and vote with collaboration/decision/vote.
================================`;

/**
 * GovernanceSource - Provides democratic participation guidance
 *
 * Budget-aware: Uses full examples for capable models, minimal for limited models.
 * Low priority: Gets trimmed first when context is tight.
 */
export class GovernanceSource implements RAGSource {
  readonly name = 'governance';

  // Low priority - governance examples are nice-to-have, not critical
  readonly priority = 20;

  // Small budget allocation - governance is boilerplate, not context-specific
  readonly defaultBudgetPercent = 5;

  isApplicable(context: RAGSourceContext): boolean {
    // Skip entirely for very limited models (< 2000 tokens)
    const modelId = context.options?.modelId;
    if (modelId) {
      const contextWindow = getContextWindow(modelId);
      if (contextWindow < 2000) {
        return false;
      }
    }
    return true;
  }

  async load(context: RAGSourceContext, allocatedBudget: number): Promise<RAGSection> {
    const startTime = Date.now();

    // Determine which version to use based on budget and model capability
    const modelId = context.options?.modelId;
    const isLimited = modelId && (isSlowLocalModel(modelId) || getContextWindow(modelId) < 8000);

    // Estimate tokens: ~4 chars per token
    const fullTokens = Math.ceil(FULL_GOVERNANCE_SECTION.length / 4);
    const minimalTokens = Math.ceil(MINIMAL_GOVERNANCE_SECTION.length / 4);

    let selectedSection: string;
    let tokenCount: number;

    if (isLimited || allocatedBudget < fullTokens) {
      // Use minimal version for limited models or tight budget
      if (allocatedBudget < minimalTokens) {
        // Can't even fit minimal - skip entirely
        return {
          sourceName: this.name,
          tokenCount: 0,
          loadTimeMs: Date.now() - startTime,
          systemPromptSection: undefined
        };
      }
      selectedSection = MINIMAL_GOVERNANCE_SECTION;
      tokenCount = minimalTokens;
    } else {
      // Use full version for capable models
      selectedSection = FULL_GOVERNANCE_SECTION;
      tokenCount = fullTokens;
    }

    return {
      sourceName: this.name,
      tokenCount,
      loadTimeMs: Date.now() - startTime,
      systemPromptSection: selectedSection
    };
  }
}
