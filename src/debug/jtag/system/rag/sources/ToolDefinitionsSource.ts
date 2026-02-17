/**
 * ToolDefinitionsSource - Budget-aware RAG source for LLM tool definitions
 *
 * This source is the SINGLE AUTHORITY for injecting tool definitions into
 * the LLM context. Previously, PersonaResponseGenerator would append tool
 * definitions AFTER the RAG budget was calculated, causing unbounded context
 * growth that crashed local models (Candle) with NaN/Inf errors.
 *
 * Behavior by provider capability:
 * - 'native' (Anthropic, OpenAI, Together, Groq): Produces metadata.nativeToolSpecs
 *   for the JSON tools[] request parameter. Budget-aware — drops lowest-priority
 *   tools if they exceed the allocated budget.
 * - 'xml' (DeepSeek, Candle, etc.): Produces systemPromptSection with
 *   XML-formatted tool definitions, prioritized then truncated to budget.
 *   Essential tools (collaboration/chat, code/*) are kept; lowest-priority dropped.
 * - 'none': Not applicable — returns nothing. Must be explicitly set.
 *
 * Priority 45 — below CodeToolSource (50, workflow guidance), above ActivityContext (40).
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import type { NativeToolSpec } from '../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import { PersonaToolRegistry } from '../../user/server/modules/PersonaToolRegistry';
import {
  getPrimaryAdapter,
  convertToNativeToolSpecs,
  supportsNativeTools,
  type ToolDefinition
} from '../../user/server/modules/ToolFormatAdapter';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('ToolDefinitionsSource', 'rag');

export class ToolDefinitionsSource implements RAGSource {
  readonly name = 'tool-definitions';
  readonly priority = 45;
  readonly defaultBudgetPercent = 10;

  isApplicable(context: RAGSourceContext): boolean {
    // Only skip tools when explicitly disabled — every AI gets tools by default
    if (context.toolCapability === 'none') {
      return false;
    }
    return true;
  }

  async load(context: RAGSourceContext, allocatedBudget: number): Promise<RAGSection> {
    const startTime = performance.now();

    try {
      // Get available tools for this persona
      const registry = PersonaToolRegistry.sharedInstance();
      const availableTools = await registry.listToolsForPersonaAsync(context.personaId);

      if (availableTools.length === 0) {
        return this.emptySection(startTime);
      }

      // Convert to adapter format
      const toolDefinitions: ToolDefinition[] = availableTools.map(t => ({
        name: t.name,
        description: t.description,
        parameters: t.parameters,
        category: t.category
      }));

      if (context.toolCapability === 'native') {
        // Native tools go in request.tools (separate from system prompt), so they
        // deserve a higher effective budget. Cloud providers have 200K+ context windows;
        // even 3000 tokens for tools is a rounding error. The RAG budget's 10% allocation
        // was designed for system prompt sections — native tools are a different beast.
        const effectiveBudget = Math.max(allocatedBudget, 3000);
        return this.loadNativeTools(context, toolDefinitions, effectiveBudget, startTime);
      } else {
        // XML tools go IN the system prompt — respect the allocated budget strictly
        return this.loadXmlTools(context, toolDefinitions, allocatedBudget, startTime);
      }
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      log.error(`Failed to load tool definitions: ${message}`);
      return this.emptySection(startTime, message);
    }
  }

  /**
   * Native tool providers (Anthropic, OpenAI, Together, Groq):
   * Produce NativeToolSpec[] in metadata for the JSON tools request parameter.
   * Budget-aware: drops lowest-priority tools if they exceed allocation.
   */
  private loadNativeTools(
    context: RAGSourceContext,
    toolDefinitions: ToolDefinition[],
    allocatedBudget: number,
    startTime: number
  ): RAGSection {
    // Exclude meta-tools — models with native tool calling don't need discovery tools.
    // search_tools/list_tools cause infinite loops where models search instead of act.
    const META_TOOLS = new Set(['search_tools', 'list_tools', 'working_memory']);
    let prioritizedTools = toolDefinitions.filter(t => !META_TOOLS.has(t.name));

    // Three-tier prioritization with budget awareness
    const recipeToolNames = this.getRecipeToolNames(context);
    const hasRecipeTools = recipeToolNames.size > 0;
    const MAX_NATIVE_TOOLS = hasRecipeTools ? 32 : 64;

    if (prioritizedTools.length > MAX_NATIVE_TOOLS) {
      prioritizedTools = this.prioritizeTools(prioritizedTools, recipeToolNames, hasRecipeTools, MAX_NATIVE_TOOLS);
    }

    // Budget check: estimate token cost of native specs
    const specs = convertToNativeToolSpecs(prioritizedTools);
    let tokenEstimate = this.estimateNativeToolTokens(specs);

    // If over budget, progressively drop lowest-priority tools
    while (tokenEstimate > allocatedBudget && prioritizedTools.length > 2) {
      prioritizedTools = prioritizedTools.slice(0, prioritizedTools.length - 3);
      const reducedSpecs = convertToNativeToolSpecs(prioritizedTools);
      tokenEstimate = this.estimateNativeToolTokens(reducedSpecs);
    }

    const finalSpecs = convertToNativeToolSpecs(prioritizedTools);
    const finalTokens = this.estimateNativeToolTokens(finalSpecs);

    log.debug(`Native tools: ${finalSpecs.length} specs (~${finalTokens} tokens) for persona ${context.personaId.slice(0, 8)}`);

    return {
      sourceName: this.name,
      tokenCount: finalTokens,
      loadTimeMs: performance.now() - startTime,
      metadata: {
        nativeToolSpecs: finalSpecs,
        toolChoice: 'auto',
        toolCount: finalSpecs.length,
        totalAvailable: toolDefinitions.length,
        budgetRespected: finalTokens <= allocatedBudget,
      },
    };
  }

  /**
   * XML tool providers (DeepSeek, Candle, etc.):
   * Produce systemPromptSection with formatted tool definitions, budget-truncated.
   *
   * CRITICAL: Prioritize BEFORE truncation. Previously, tools were truncated from
   * the end of an alphabetically-sorted list, keeping useless tools (adapter/*)
   * and dropping essential ones (collaboration/chat/send, code/*). Now we put
   * essential tools first so budget truncation drops the least important ones.
   */
  private loadXmlTools(
    context: RAGSourceContext,
    toolDefinitions: ToolDefinition[],
    allocatedBudget: number,
    startTime: number
  ): RAGSection {
    // Prioritize BEFORE formatting — essential tools first, rest at end.
    // This ensures budget truncation drops lowest-priority tools, not essential ones.
    const recipeToolNames = this.getRecipeToolNames(context);
    let prioritized = this.prioritizeTools(
      toolDefinitions,
      recipeToolNames,
      recipeToolNames.size > 0,
      toolDefinitions.length  // No cap yet — budget handles the limiting
    );

    const adapter = getPrimaryAdapter();
    const formattedTools = adapter.formatToolsForPrompt(prioritized);

    const toolsSection = `\n\n=== AVAILABLE TOOLS ===\nYou have access to the following tools:\n\n${formattedTools}\n================================`;

    let finalSection = toolsSection;
    let tokenCount = this.estimateTokens(toolsSection);
    let finalToolCount = prioritized.length;

    // Budget truncation: progressively drop lowest-priority tools (at end of list)
    // Minimum 2 tools (critical: chat/send + chat/history) — not 5, which is too
    // many for tight Candle budgets (~250 tokens for tools).
    if (tokenCount > allocatedBudget && prioritized.length > 2) {
      let reducedTools = prioritized;
      while (tokenCount > allocatedBudget && reducedTools.length > 2) {
        // Drop 3 at a time for faster convergence, or 1 when close to minimum
        const dropCount = reducedTools.length > 8 ? 3 : 1;
        reducedTools = reducedTools.slice(0, reducedTools.length - dropCount);
        const reduced = adapter.formatToolsForPrompt(reducedTools);
        finalSection = `\n\n=== AVAILABLE TOOLS ===\nYou have access to the following tools:\n\n${reduced}\n================================`;
        tokenCount = this.estimateTokens(finalSection);
      }
      finalToolCount = reducedTools.length;
    }

    log.debug(`XML tools: ${finalToolCount}/${toolDefinitions.length} tools (~${tokenCount} tokens, budget=${allocatedBudget})`);

    return {
      sourceName: this.name,
      tokenCount,
      loadTimeMs: performance.now() - startTime,
      systemPromptSection: finalSection,
      metadata: {
        toolCount: finalToolCount,
        totalAvailable: toolDefinitions.length,
        format: 'xml',
        budgetRespected: tokenCount <= allocatedBudget,
      },
    };
  }

  /**
   * Four-tier tool prioritization with sub-ordering:
   * 1. Recipe tools (activity's core toolset — go FIRST)
   * 2. Critical tools (chat communication — bare minimum)
   * 3. Essential tools (code, data, decisions — ordered by importance)
   * 4. Everything else (fill remaining slots)
   *
   * Within essentials, tools are ordered by PREFIX PRIORITY so budget
   * truncation (which drops from the end) removes the least important:
   *   collaboration/chat > code > collaboration/decision > data > ai
   *
   * This ensures that when tight budgets (e.g., Candle 2-tool limit) kick in,
   * AIs get collaboration/chat/send instead of ai/adapter/test.
   */
  private prioritizeTools(
    tools: ToolDefinition[],
    recipeToolNames: Set<string>,
    hasRecipeTools: boolean,
    maxTools: number
  ): ToolDefinition[] {
    // Critical tools that should ALWAYS survive budget truncation
    const CRITICAL_TOOLS = new Set([
      'collaboration/chat/send', 'collaboration/chat/history',
    ]);

    // Essential prefix ordering — most important first.
    // When budget truncation drops from the end, ai/* goes first, then data/*, etc.
    const ESSENTIAL_PREFIX_ORDER: string[] = hasRecipeTools
      ? [] // When recipe tools exist, only allow exact essential matches
      : [
          'collaboration/chat/',     // Communication is #1
          'code/',                   // Code abilities are #2
          'collaboration/decision/', // Decision making #3
          'collaboration/wall/',     // Shared documents #4
          'data/',                   // Data access #5
          'ai/',                     // AI meta-tools #6 (least important essential)
        ];

    const recipe: ToolDefinition[] = [];
    const critical: ToolDefinition[] = [];
    const essential: ToolDefinition[] = [];
    const rest: ToolDefinition[] = [];

    for (const tool of tools) {
      if (recipeToolNames.has(tool.name)) {
        recipe.push(tool);
      } else if (CRITICAL_TOOLS.has(tool.name)) {
        critical.push(tool);
      } else if (ESSENTIAL_PREFIX_ORDER.some(p => tool.name.startsWith(p))) {
        essential.push(tool);
      } else {
        rest.push(tool);
      }
    }

    // Sort essentials by prefix priority (most important prefix first)
    essential.sort((a, b) => {
      const aIdx = ESSENTIAL_PREFIX_ORDER.findIndex(p => a.name.startsWith(p));
      const bIdx = ESSENTIAL_PREFIX_ORDER.findIndex(p => b.name.startsWith(p));
      const aPri = aIdx >= 0 ? aIdx : ESSENTIAL_PREFIX_ORDER.length;
      const bPri = bIdx >= 0 ? bIdx : ESSENTIAL_PREFIX_ORDER.length;
      if (aPri !== bPri) return aPri - bPri;
      return a.name.localeCompare(b.name);
    });

    const remaining = maxTools - recipe.length - critical.length - essential.length;
    const result = [...recipe, ...critical, ...essential, ...rest.slice(0, Math.max(0, remaining))];

    log.debug(`Tool prioritization: ${recipe.length} recipe + ${critical.length} critical + ${essential.length} essential + ${Math.max(0, remaining)} general = ${result.length} (from ${tools.length} total, cap=${maxTools})`);

    return result;
  }

  /**
   * Extract recipe tool names from RAG context options.
   * Recipe tools are loaded separately by ChatRAGBuilder, but we can access
   * them from the context's recipeTools if threaded through.
   */
  private getRecipeToolNames(context: RAGSourceContext): Set<string> {
    // Recipe tools may be available on the context options
    const recipeTools = (context.options as any)?.recipeTools;
    if (!recipeTools || !Array.isArray(recipeTools)) {
      return new Set();
    }
    return new Set(
      recipeTools
        .filter((t: any) => t.enabledFor?.includes('ai'))
        .map((t: any) => t.name)
    );
  }

  /**
   * Estimate token count for native tool specs.
   * Native specs are sent as JSON in the request body, consuming context window.
   */
  private estimateNativeToolTokens(specs: NativeToolSpec[]): number {
    // JSON.stringify approximation: ~4 chars per token
    return Math.ceil(JSON.stringify(specs).length / 4);
  }

  private estimateTokens(text: string): number {
    return Math.ceil(text.length / 4);
  }

  private emptySection(startTime: number, error?: string): RAGSection {
    return {
      sourceName: this.name,
      tokenCount: 0,
      loadTimeMs: performance.now() - startTime,
      metadata: error ? { error } : { toolCount: 0 },
    };
  }
}
