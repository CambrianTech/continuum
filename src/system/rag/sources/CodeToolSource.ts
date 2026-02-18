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
   * Full coding methodology prompt — injected into system prompt.
   * Only includes workflow steps for tool groups the persona has access to.
   *
   * Inspired by Claude Code, Aider, SWE-Agent, and OpenCode system prompts.
   * These tools achieve 75-85% on SWE-bench because their workflow guidance
   * is extremely specific about HOW to use tools, not just WHAT tools exist.
   */
  private buildFullPrompt(context: RAGSourceContext): string {
    const registry = PersonaToolRegistry.sharedInstance();
    const tools = registry.listToolsForPersona(context.personaId);
    const codeTools = tools.filter(t => t.name.startsWith('code/'));

    // Determine which capabilities are available
    const hasDiscovery = codeTools.some(t => t.name === 'code/tree' || t.name === 'code/search');
    const hasRead = codeTools.some(t => t.name === 'code/read');
    const hasWrite = codeTools.some(t => t.name === 'code/write');
    const hasEdit = codeTools.some(t => t.name === 'code/edit');
    const hasVerify = codeTools.some(t => t.name === 'code/verify');
    const hasDiff = codeTools.some(t => t.name === 'code/diff');
    const hasUndo = codeTools.some(t => t.name === 'code/undo');
    const hasGit = codeTools.some(t => t.name === 'code/git');
    const hasShell = codeTools.some(t => t.name === 'code/shell/execute');
    const hasHistory = codeTools.some(t => t.name === 'code/history');

    const sections: string[] = [];

    // ── Core workflow ─────────────────────────────────────────
    sections.push(`## Code Editing Methodology

### Workflow: Orient → Read → Edit → Verify → Iterate`);

    const steps: string[] = [];
    if (hasDiscovery) steps.push('1. **Orient** — code/tree to see project structure, code/search to find relevant files and patterns');
    if (hasRead) steps.push(`${steps.length + 1}. **Read** — code/read the file you want to change. Understand context around the target lines`);
    if (hasEdit) steps.push(`${steps.length + 1}. **Edit** — code/edit with search_replace for surgical changes. Match text EXACTLY as it appears in code/read output`);
    else if (hasWrite) steps.push(`${steps.length + 1}. **Write** — code/write for new files or full replacements`);
    if (hasVerify) steps.push(`${steps.length + 1}. **Verify** — code/verify after EVERY change. If it fails: read errors, fix, verify again. Never move on with broken code`);
    if (hasDiff || hasGit) {
      const parts = [];
      if (hasDiff) parts.push('code/diff to preview changes');
      if (hasGit) parts.push('code/git status before committing');
      steps.push(`${steps.length + 1}. **Review** — ${parts.join(', ')}`);
    }
    sections.push(steps.join('\n'));

    // ── Critical rules ────────────────────────────────────────
    const rules: string[] = [];

    if (hasRead && (hasWrite || hasEdit)) {
      rules.push('- **ALWAYS read before editing.** You MUST code/read a file before using code/edit or code/write on it. Editing without reading leads to wrong assumptions and broken code');
    }

    if (hasEdit) {
      rules.push(`- **code/edit search_replace rules:**
  - The \`search\` text must match EXACTLY — character for character, including whitespace and indentation
  - Include enough surrounding context to make the search text unique in the file
  - If the edit fails (search text not found), code/read the file again — it may have changed
  - Use code/edit for modifications. Only use code/write for NEW files that don't exist yet
  - Prefer small, focused edits over rewriting entire files`);
    }

    if (hasWrite && hasEdit) {
      rules.push('- **Prefer code/edit over code/write** for existing files. code/write replaces the ENTIRE file — one mistake and all content is lost. code/edit is surgical');
    }

    if (hasDiscovery) {
      rules.push('- **Use code/search, not shell grep.** code/search is optimized for codebase search with regex. Use it to find patterns, references, and definitions');
      rules.push('- **Understand existing patterns first.** Before creating something new, code/search for similar implementations. Follow existing conventions');
    }

    if (hasVerify) {
      rules.push('- **Fix errors immediately.** When code/verify fails, READ the error output carefully. code/read the failing file, understand the issue, fix it, verify again. Never leave broken code');
    }

    if (hasUndo) {
      rules.push('- **code/undo is your safety net.** Every edit is tracked. If something goes wrong, undo it');
    }

    if (hasShell) {
      rules.push('- **Use code tools for file operations.** Use code/read instead of shell cat/head. Use code/search instead of shell grep. Use code/tree instead of shell ls/find. Reserve code/shell/execute for build, test, and system commands');
    }

    if (rules.length > 0) {
      sections.push(`\n### Critical Rules\n${rules.join('\n')}`);
    }

    // ── Anti-patterns ─────────────────────────────────────────
    if (hasWrite || hasEdit) {
      sections.push(`\n### What NOT To Do
- Editing a file you haven't read — your search text won't match
- Rewriting entire files with code/write when code/edit would suffice
- Making multiple edits before verifying — verify after EACH change
- Guessing at file paths — use code/tree and code/search to find them
- Leaving code that doesn't compile — always verify
- Using shell commands for file reading/searching when code tools exist`);
    }

    return sections.join('\n');
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
