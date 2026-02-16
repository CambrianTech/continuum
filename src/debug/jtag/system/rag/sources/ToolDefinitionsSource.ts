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
 * - 'xml' (DeepSeek): Produces systemPromptSection with XML-formatted tool
 *   definitions, truncated to budget.
 * - 'none' (Candle, Ollama, xAI, Fireworks): Not applicable — returns nothing.
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
    // No tool definitions for providers that can't call tools
    if (context.toolCapability === 'none' || !context.toolCapability) {
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
        return this.loadNativeTools(context, toolDefinitions, allocatedBudget, startTime);
      } else if (context.toolCapability === 'xml') {
        return this.loadXmlTools(toolDefinitions, allocatedBudget, startTime);
      }

      return this.emptySection(startTime);
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
    while (tokenEstimate > allocatedBudget && prioritizedTools.length > 5) {
      prioritizedTools = prioritizedTools.slice(0, prioritizedTools.length - 5);
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
   * XML tool providers (DeepSeek):
   * Produce systemPromptSection with formatted tool definitions, budget-truncated.
   */
  private loadXmlTools(
    toolDefinitions: ToolDefinition[],
    allocatedBudget: number,
    startTime: number
  ): RAGSection {
    const adapter = getPrimaryAdapter();
    const formattedTools = adapter.formatToolsForPrompt(toolDefinitions);

    const toolsSection = `\n\n=== AVAILABLE TOOLS ===\nYou have access to the following tools that you can use during your responses:\n\n${formattedTools}\n\nThe tool will be executed and results will be provided for you to analyze and respond to.\n================================`;

    let finalSection = toolsSection;
    let tokenCount = this.estimateTokens(toolsSection);

    // Budget truncation: if over budget, reduce tool count
    if (tokenCount > allocatedBudget && toolDefinitions.length > 5) {
      // Progressively reduce tools until we fit
      let reducedTools = toolDefinitions;
      while (tokenCount > allocatedBudget && reducedTools.length > 5) {
        reducedTools = reducedTools.slice(0, reducedTools.length - 5);
        const reduced = adapter.formatToolsForPrompt(reducedTools);
        finalSection = `\n\n=== AVAILABLE TOOLS ===\nYou have access to the following tools:\n\n${reduced}\n================================`;
        tokenCount = this.estimateTokens(finalSection);
      }
    }

    log.debug(`XML tools: ${toolDefinitions.length} tools (~${tokenCount} tokens)`);

    return {
      sourceName: this.name,
      tokenCount,
      loadTimeMs: performance.now() - startTime,
      systemPromptSection: finalSection,
      metadata: {
        toolCount: toolDefinitions.length,
        format: 'xml',
        budgetRespected: tokenCount <= allocatedBudget,
      },
    };
  }

  /**
   * Three-tier tool prioritization (same logic as PersonaResponseGenerator):
   * 1. Recipe tools (activity's core toolset — go FIRST)
   * 2. Essentials (bare minimum for coordination)
   * 3. Everything else (fill remaining slots)
   */
  private prioritizeTools(
    tools: ToolDefinition[],
    recipeToolNames: Set<string>,
    hasRecipeTools: boolean,
    maxTools: number
  ): ToolDefinition[] {
    const ESSENTIAL_TOOLS = new Set([
      'collaboration/chat/send', 'collaboration/chat/history',
      'collaboration/decision/propose', 'collaboration/decision/vote',
    ]);
    const essentialPrefixes = hasRecipeTools
      ? [] // When recipe tools exist, only allow exact essential matches
      : ['collaboration/chat/', 'collaboration/decision/', 'data/', 'ai/'];

    const recipe: ToolDefinition[] = [];
    const essential: ToolDefinition[] = [];
    const rest: ToolDefinition[] = [];

    for (const tool of tools) {
      if (recipeToolNames.has(tool.name)) {
        recipe.push(tool);
      } else if (ESSENTIAL_TOOLS.has(tool.name) ||
                 essentialPrefixes.some(p => tool.name.startsWith(p))) {
        essential.push(tool);
      } else {
        rest.push(tool);
      }
    }

    const remaining = maxTools - recipe.length - essential.length;
    const result = [...recipe, ...essential, ...rest.slice(0, Math.max(0, remaining))];

    log.debug(`Tool prioritization: ${recipe.length} recipe + ${essential.length} essential + ${Math.max(0, remaining)} general = ${result.length} (from ${tools.length} total, cap=${maxTools})`);

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
