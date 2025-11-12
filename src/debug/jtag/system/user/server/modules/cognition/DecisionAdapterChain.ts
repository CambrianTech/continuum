/**
 * DecisionAdapterChain - Chain of Responsibility for decision-making
 *
 * Coordinates multiple decision adapters in priority order:
 * 1. FastPathAdapter (priority 100) - Mentions, always respond
 * 2. ThermalAdapter (priority 50) - Temperature-based gating
 * 3. LLMAdapter (priority 10) - Fallback LLM gating
 *
 * Philosophy: Try fast/simple adapters first, fall back to expensive/complex ones.
 * Each adapter can say "I'll handle this" or "pass to next adapter".
 */

import type { IDecisionAdapter, DecisionContext, CognitiveDecision } from './adapters/IDecisionAdapter';
import type { BaseEntity } from '../../../../data/entities/BaseEntity';
import { FastPathAdapter } from './adapters/FastPathAdapter';
import { ThermalAdapter } from './adapters/ThermalAdapter';
import { LLMAdapter } from './adapters/LLMAdapter';

export class DecisionAdapterChain {
  private adapters: IDecisionAdapter[] = [];

  constructor() {
    // Register adapters in priority order (high to low)
    this.registerAdapter(new FastPathAdapter());
    this.registerAdapter(new ThermalAdapter());
    this.registerAdapter(new LLMAdapter());
  }

  /**
   * Register an adapter (maintains priority order)
   */
  private registerAdapter(adapter: IDecisionAdapter): void {
    this.adapters.push(adapter);
    // Sort by priority (descending - highest first)
    this.adapters.sort((a, b) => b.priority - a.priority);
  }

  /**
   * Process decision through adapter chain
   *
   * Tries each adapter in priority order until one returns a decision.
   * Returns the decision from the first adapter that handles it (non-null).
   */
  async processDecision<TEvent extends BaseEntity>(
    context: DecisionContext<TEvent>
  ): Promise<CognitiveDecision> {
    console.log(`🔗 DecisionAdapterChain: Processing decision for ${context.personaDisplayName}`);
    console.log(`   Event: ${context.eventContent.slice(0, 60)}...`);
    console.log(`   isMentioned: ${context.isMentioned}, senderIsHuman: ${context.senderIsHuman}`);

    for (const adapter of this.adapters) {
      console.log(`   🔍 Trying adapter: ${adapter.name} (priority ${adapter.priority})`);

      // Let adapter evaluate the decision
      const decision = await adapter.evaluate(context);

      if (decision !== null) {
        console.log(`   ✅ ${adapter.name} handled decision: ${decision.shouldRespond ? 'RESPOND' : 'SILENT'} (confidence: ${decision.confidence.toFixed(2)})`);
        console.log(`   💭 Reason: ${decision.reason}`);

        return decision;
      } else {
        console.log(`   ⏭️  ${adapter.name} returned null - trying next adapter`);
      }
    }

    // Should never reach here (LLMAdapter always returns non-null as fallback)
    console.error(`❌ DecisionAdapterChain: No adapter handled the decision! This should not happen.`);

    // Return safe default
    return {
      shouldRespond: false,
      confidence: 0.0,
      reason: 'No adapter handled the decision (error state)',
      model: 'None'
    };
  }

  /**
   * Get adapter by name (for testing/debugging)
   */
  getAdapter(name: string): IDecisionAdapter | undefined {
    return this.adapters.find(a => a.name === name);
  }

  /**
   * Get all adapters (for inspection)
   */
  getAllAdapters(): IDecisionAdapter[] {
    return [...this.adapters];
  }
}
