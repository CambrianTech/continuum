/**
 * WidgetContextSource - Loads UI state from Positron for RAG context
 *
 * Provides AI awareness of what the user is currently looking at:
 * - Active widget/page
 * - Form state
 * - Selected items
 * - Browser URL (if co-browsing)
 *
 * Part of the Positron system - "so AIs can see what users see"
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import { WidgetContextService } from '../services/WidgetContextService';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('WidgetContextSource', 'rag');

export class WidgetContextSource implements RAGSource {
  readonly name = 'widget-context';
  readonly priority = 75;  // High - UI context is very relevant
  readonly defaultBudgetPercent = 10;

  isApplicable(context: RAGSourceContext): boolean {
    // Need either pre-formatted context or session ID
    return !!(context.options.widgetContext || context.sessionId);
  }

  async load(context: RAGSourceContext, _allocatedBudget: number): Promise<RAGSection> {
    const startTime = performance.now();

    try {
      // Ensure service is initialized
      WidgetContextService.initialize();

      let widgetContext: string | null = null;

      // Priority 1: Pre-formatted context from options
      if (context.options.widgetContext) {
        widgetContext = context.options.widgetContext;
        log.debug('Using widget context from options');
      }
      // Priority 2: Look up by session ID
      else if (context.sessionId) {
        widgetContext = WidgetContextService.toRAGContext(context.sessionId);
        if (widgetContext) {
          log.debug(`Loaded widget context for session ${context.sessionId.slice(0, 8)}`);
        }
      }
      // Priority 3: Most recent context (fallback)
      else {
        widgetContext = WidgetContextService.toRAGContext();
        if (widgetContext) {
          log.debug('Using most recent widget context (fallback)');
        }
      }

      if (!widgetContext) {
        return this.emptySection(startTime);
      }

      const loadTimeMs = performance.now() - startTime;
      const tokenCount = this.estimateTokens(widgetContext);

      return {
        sourceName: this.name,
        tokenCount,
        loadTimeMs,
        systemPromptSection: this.formatAsSystemPromptSection(widgetContext),
        metadata: {
          hasContext: true,
          sessionId: context.sessionId
        }
      };
    } catch (error: any) {
      log.error(`Failed to load widget context: ${error.message}`);
      return this.emptySection(startTime, error.message);
    }
  }

  private formatAsSystemPromptSection(context: string): string {
    return `
## Current User Interface State

The user is currently viewing:
${context}

Use this context to provide relevant, contextual assistance.
`.trim();
  }

  private emptySection(startTime: number, error?: string): RAGSection {
    return {
      sourceName: this.name,
      tokenCount: 0,
      loadTimeMs: performance.now() - startTime,
      metadata: error ? { error } : { hasContext: false }
    };
  }

  private estimateTokens(text: string): number {
    return Math.ceil(text.length / 4);
  }
}
