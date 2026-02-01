/**
 * CodeToolSource - Injects coding workflow awareness into persona RAG context
 *
 * Gives personas strategic awareness of the code/* command suite:
 * - When and how to use code tools (workflow patterns)
 * - Best practices (read before edit, preview with diff, undo on failure)
 * - Available code/* commands grouped by purpose
 *
 * Does NOT duplicate tool listings — ToolRegistry already provides a compact
 * list of all tools. This source provides the "how to code effectively" layer.
 *
 * Priority 50 - Medium. Valuable context for coding tasks, but not critical
 * for conversational interactions. Token cost is low (~200 tokens).
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import { PersonaToolRegistry } from '../../user/server/modules/PersonaToolRegistry';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('CodeToolSource', 'rag');

/**
 * Code tool categories for workflow documentation.
 * Each group maps to a workflow step that only appears if the persona has
 * at least one of the group's commands.
 */
interface CodeToolGroup {
  readonly label: string;
  readonly commands: string[];
  readonly hint: string;
  readonly workflowStep: string;
}

/**
 * Static code tool groups — the workflow map for personas.
 * workflowStep is the numbered instruction shown in the workflow.
 */
const CODE_TOOL_GROUPS: readonly CodeToolGroup[] = [
  {
    label: 'Discovery',
    commands: ['code/tree', 'code/search'],
    hint: 'Understand the codebase structure before making changes.',
    workflowStep: '**Discover** — Use code/tree and code/search to understand structure',
  },
  {
    label: 'Reading',
    commands: ['code/read'],
    hint: 'Read file contents and line ranges. Always read before editing.',
    workflowStep: '**Read** — Always read files before editing (code/read)',
  },
  {
    label: 'Writing',
    commands: ['code/write', 'code/edit'],
    hint: 'Create files or edit with search-replace, line-range, insert, or append.',
    workflowStep: '**Edit** — Apply changes with code/write or code/edit',
  },
  {
    label: 'Review',
    commands: ['code/diff'],
    hint: 'Preview edits as unified diff before applying. Use this to verify correctness.',
    workflowStep: '**Preview** — Use code/diff to see your changes before applying',
  },
  {
    label: 'History',
    commands: ['code/undo', 'code/history'],
    hint: 'Undo changes or view the change graph. Every edit is tracked.',
    workflowStep: '**Undo** — If something breaks, code/undo reverts any change',
  },
] as const;

export class CodeToolSource implements RAGSource {
  readonly name = 'code-tools';
  readonly priority = 50;  // Medium — below conversation/widget, above learning config
  readonly defaultBudgetPercent = 5;

  private static _cachedPrompt: string | null = null;
  private static _cacheGeneratedAt = 0;
  private static readonly CACHE_TTL_MS = 10 * 60 * 1000; // 10 minutes

  isApplicable(context: RAGSourceContext): boolean {
    // Only include if persona has at least one code/* permission
    const registry = PersonaToolRegistry.sharedInstance();
    const tools = registry.listToolsForPersona(context.personaId);
    return tools.some(t => t.name.startsWith('code/'));
  }

  async load(context: RAGSourceContext, allocatedBudget: number): Promise<RAGSection> {
    const startTime = performance.now();

    try {
      const prompt = this.getOrBuildPrompt(context);

      // Respect budget — if prompt exceeds allocation, return a minimal version
      const tokenCount = this.estimateTokens(prompt);
      const budgetTokens = Math.floor(allocatedBudget);

      const finalPrompt = tokenCount > budgetTokens
        ? this.buildMinimalPrompt()
        : prompt;

      const finalTokens = this.estimateTokens(finalPrompt);

      log.debug(`Loaded code tool guidance (${finalTokens} tokens) for persona ${context.personaId.slice(0, 8)}`);

      return {
        sourceName: this.name,
        tokenCount: finalTokens,
        loadTimeMs: performance.now() - startTime,
        systemPromptSection: finalPrompt,
        metadata: {
          codeToolCount: this.countCodeTools(context),
          budgetRespected: finalTokens <= budgetTokens,
        },
      };
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      log.error(`Failed to load code tool context: ${message}`);
      return this.emptySection(startTime, message);
    }
  }

  /**
   * Build or retrieve cached prompt
   */
  private getOrBuildPrompt(context: RAGSourceContext): string {
    const now = Date.now();
    if (
      CodeToolSource._cachedPrompt &&
      (now - CodeToolSource._cacheGeneratedAt) < CodeToolSource.CACHE_TTL_MS
    ) {
      return CodeToolSource._cachedPrompt;
    }

    const prompt = this.buildFullPrompt(context);
    CodeToolSource._cachedPrompt = prompt;
    CodeToolSource._cacheGeneratedAt = now;
    return prompt;
  }

  /**
   * Full coding workflow prompt — injected into system prompt.
   * Only includes workflow steps for tool groups the persona has access to.
   */
  private buildFullPrompt(context: RAGSourceContext): string {
    const registry = PersonaToolRegistry.sharedInstance();
    const tools = registry.listToolsForPersona(context.personaId);
    const codeTools = tools.filter(t => t.name.startsWith('code/'));

    // Filter to groups where persona has at least one command
    const availableGroups: { group: CodeToolGroup; available: string[] }[] = [];
    for (const group of CODE_TOOL_GROUPS) {
      const available = group.commands.filter(cmd =>
        codeTools.some(t => t.name === cmd)
      );
      if (available.length > 0) {
        availableGroups.push({ group, available });
      }
    }

    // Build numbered workflow steps (only for groups persona has)
    const workflowSteps = availableGroups
      .map((entry, i) => `${i + 1}. ${entry.group.workflowStep}`)
      .join('\n');

    // Build grouped tool listing
    const groupLines = availableGroups
      .map(entry => `${entry.group.label}: ${entry.available.join(', ')} — ${entry.group.hint}`)
      .join('\n');

    const hasWriteTools = codeTools.some(t => t.name === 'code/write' || t.name === 'code/edit');

    return `## Coding Capabilities

You have access to workspace code tools. Follow this workflow for coding tasks:

${workflowSteps}

${groupLines}
${hasWriteTools ? '\nEvery write/edit is tracked in a change graph with full undo support.\nNever edit blind — always read first, diff to preview, then apply.' : ''}`.trim();
  }

  /**
   * Minimal prompt when budget is tight — just list available tool names
   */
  private buildMinimalPrompt(): string {
    // List all known code commands from the groups (static — no registry call needed)
    const allCommands = CODE_TOOL_GROUPS.flatMap(g => g.commands);
    return `Code tools available: ${allCommands.join(', ')}. Read before editing. Use code/diff to preview.`;
  }

  private countCodeTools(context: RAGSourceContext): number {
    const registry = PersonaToolRegistry.sharedInstance();
    const tools = registry.listToolsForPersona(context.personaId);
    return tools.filter(t => t.name.startsWith('code/')).length;
  }

  private emptySection(startTime: number, error?: string): RAGSection {
    return {
      sourceName: this.name,
      tokenCount: 0,
      loadTimeMs: performance.now() - startTime,
      metadata: error ? { error } : { hasCodeTools: false },
    };
  }

  private estimateTokens(text: string): number {
    return Math.ceil(text.length / 4);
  }
}
