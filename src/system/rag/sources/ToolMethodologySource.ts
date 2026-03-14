/**
 * ToolMethodologySource - Workflow guidance for non-code tools
 *
 * CodeToolSource handles code/* methodology. This source handles everything else:
 * - Documentation tools (utilities/docs/*)
 * - Collaboration tools (collaboration/chat/*, wall/*, decision/*)
 * - Data tools (data/*)
 * - Sentinel tools (sentinel/*)
 *
 * Without this, personas know WHAT tools exist (from ToolDefinitionsSource)
 * but not WHEN or HOW to use them effectively. They get a one-line "USE tools"
 * instruction and no workflow guidance.
 *
 * Priority 48 - Just below CodeToolSource (50), above ToolDefinitionsSource (45).
 * Budget 3% (~120 tokens). Conditional: only includes guidance for tool
 * categories the persona actually has.
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import { PersonaToolRegistry } from '../../user/server/modules/PersonaToolRegistry';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('ToolMethodologySource', 'rag');

/** Tool category with its workflow guidance */
interface ToolCategory {
  readonly prefix: string;
  readonly label: string;
  readonly guidance: string;
}

/** Workflow guidance per tool category */
const TOOL_CATEGORIES: readonly ToolCategory[] = [
  {
    prefix: 'utilities/docs/',
    label: 'Documentation',
    guidance: 'Search → Read TOC → Read Section. Use utilities/docs/search --pattern="keyword" to find relevant docs. Then utilities/docs/read --doc="name" --toc for overview. Then --section="Title" for detail. Never read entire documents.',
  },
  {
    prefix: 'collaboration/chat/',
    label: 'Chat',
    guidance: 'Call collaboration/chat/send to communicate with others. Call collaboration/chat/export to read recent messages. Use --replyToId for threaded replies.',
  },
  {
    prefix: 'collaboration/wall/',
    label: 'Wall',
    guidance: 'Use collaboration/wall/write to create shared documents visible to all room members. wall/read to check existing content. Walls are persistent — use for notes, plans, summaries.',
  },
  {
    prefix: 'decision/',
    label: 'Decisions',
    guidance: 'Use decision/propose for group decisions, decision/vote to participate, decision/status to check results. Democratic process — propose and let the group decide.',
  },
  {
    prefix: 'data/',
    label: 'Data',
    guidance: 'Use data/list with --collection and --filter for queries. data/read --collection --id for single entities. Always specify collection name.',
  },
  {
    prefix: 'sentinel/',
    label: 'Sentinel',
    guidance: 'Call sentinel/coding-agent for ANY coding task — it uses Claude Code to read, edit, and verify code across multiple files. Call sentinel/run for custom pipelines. Call sentinel/status to check progress.',
  },
] as const;

export class ToolMethodologySource implements RAGSource {
  readonly name = 'tool-methodology';
  readonly priority = 48;
  readonly defaultBudgetPercent = 3;

  isApplicable(context: RAGSourceContext): boolean {
    // Skip if persona has no tools at all
    if (context.toolCapability === 'none') return false;

    // Check if persona has any non-code tools that we provide guidance for
    const registry = PersonaToolRegistry.sharedInstance();
    const tools = registry.listToolsForPersona(context.personaId);
    return tools.some(t => TOOL_CATEGORIES.some(cat => t.name.startsWith(cat.prefix)));
  }

  async load(context: RAGSourceContext, allocatedBudget: number): Promise<RAGSection> {
    const startTime = performance.now();

    try {
      const prompt = this.buildPrompt(context);
      const tokenCount = this.estimateTokens(prompt);
      const budgetTokens = Math.floor(allocatedBudget);

      const finalPrompt = tokenCount > budgetTokens
        ? this.buildMinimalPrompt(context)
        : prompt;

      const finalTokens = this.estimateTokens(finalPrompt);

      log.debug(`Loaded tool methodology (${finalTokens} tokens) for persona ${context.personaId.slice(0, 8)}`);

      return {
        sourceName: this.name,
        tokenCount: finalTokens,
        loadTimeMs: performance.now() - startTime,
        systemPromptSection: finalPrompt,
        metadata: {
          budgetRespected: finalTokens <= budgetTokens,
        },
      };
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      log.error(`Failed to load tool methodology: ${message}`);
      return {
        sourceName: this.name,
        tokenCount: 0,
        loadTimeMs: performance.now() - startTime,
        metadata: { error: message },
      };
    }
  }

  private buildPrompt(context: RAGSourceContext): string {
    const registry = PersonaToolRegistry.sharedInstance();
    const tools = registry.listToolsForPersona(context.personaId);

    const sections: string[] = ['## Tool Methodology'];

    for (const category of TOOL_CATEGORIES) {
      const hasCategory = tools.some(t => t.name.startsWith(category.prefix));
      if (hasCategory) {
        sections.push(`### ${category.label}\n${category.guidance}`);
      }
    }

    return sections.join('\n');
  }

  private buildMinimalPrompt(context: RAGSourceContext): string {
    const registry = PersonaToolRegistry.sharedInstance();
    const tools = registry.listToolsForPersona(context.personaId);

    const activeLabels = TOOL_CATEGORIES
      .filter(cat => tools.some(t => t.name.startsWith(cat.prefix)))
      .map(cat => cat.label);

    return `Tool categories: ${activeLabels.join(', ')}. Use utilities/docs/search to find docs. Use data/list with --collection and --filter for queries.`;
  }

  private estimateTokens(text: string): number {
    return Math.ceil(text.length / 4);
  }
}
