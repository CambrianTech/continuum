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
import { CognitionLogger } from './CognitionLogger';
import type { AdapterDecision, DecisionContextMetadata } from '../../../../data/entities/AdapterDecisionLogEntity';

export class DecisionAdapterChain {
  private adapters: IDecisionAdapter[] = [];
  private log: (message: string, ...args: any[]) => void;

  constructor(logger?: (message: string, ...args: any[]) => void) {
    // Default to console.log if no logger provided (for tests)
    this.log = logger || console.log.bind(console);

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
    this.log(`🔗 DecisionAdapterChain: Processing decision for ${context.personaDisplayName}`);
    // Defensive: eventContent could be undefined if message content was null
    const eventPreview = context.eventContent?.slice(0, 60) ?? '[no content]';
    this.log(`   Event: ${eventPreview}...`);
    this.log(`   isMentioned: ${context.isMentioned}, senderIsHuman: ${context.senderIsHuman}`);

    for (const adapter of this.adapters) {
      this.log(`   🔍 Trying adapter: ${adapter.name} (priority ${adapter.priority})`);

      // Let adapter evaluate the decision
      const startTime = Date.now();
      const decision = await adapter.evaluate(context);
      const evaluationDurationMs = Date.now() - startTime;

      // Build decision context metadata for logging
      const decisionContextMetadata: DecisionContextMetadata = {
        messageText: context.eventContent?.slice(0, 200) ?? '[no content]',
        priority: 0.8,  // Priority for decision-making
        isMentioned: context.isMentioned,
        senderIsHuman: context.senderIsHuman
      };

      // Extract contextId from trigger event (domain-specific)
      // For chat: roomId, for other domains: appropriate context identifier
      const contextId = (context.triggerEvent as any).roomId || (context.triggerEvent as any).contextId || context.personaId;

      if (decision !== null) {
        this.log(`   ✅ ${adapter.name} handled decision: ${decision.shouldRespond ? 'RESPOND' : 'SILENT'} (confidence: ${decision.confidence.toFixed(2)})`);
        this.log(`   💭 Reason: ${decision.reason}`);

        // Log adapter decision to cognition database
        const adapterDecision: AdapterDecision = decision.shouldRespond ? 'RESPOND' : 'SILENT';
        await CognitionLogger.logAdapterDecision(
          context.personaId,
          context.personaDisplayName,
          adapter.name,
          adapterDecision,
          decision.confidence,
          decision.reason,
          decisionContextMetadata,
          evaluationDurationMs,
          'chat',  // Domain
          contextId
        );

        return decision;
      } else {
        this.log(`   ⏭️  ${adapter.name} returned null - trying next adapter`);

        // Log PASS decision (adapter chose not to handle)
        await CognitionLogger.logAdapterDecision(
          context.personaId,
          context.personaDisplayName,
          adapter.name,
          'PASS',
          0.0,
          'Adapter passed - did not handle this case',
          decisionContextMetadata,
          evaluationDurationMs,
          'chat',
          contextId
        );
      }
    }

    // Should never reach here (LLMAdapter always returns non-null as fallback)
    this.log(`❌ DecisionAdapterChain: No adapter handled the decision! This should not happen.`);

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
