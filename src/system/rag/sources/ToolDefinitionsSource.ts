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
import type { RecipeToolDeclaration } from '../../recipes/shared/RecipeTypes';
import { PersonaToolRegistry } from '../../user/server/modules/PersonaToolRegistry';
import {
  getPrimaryAdapter,
  convertToNativeToolSpecs,
  supportsNativeTools,
  type ToolDefinition
} from '../../user/server/modules/ToolFormatAdapter';
import { ToolGroupRegistry } from './ToolGroupRegistry';
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

    // Exclude chat/send when responding in a chat room — the text response IS the message.
    // Including it causes models (especially Groq/Llama) to call chat/send redundantly,
    // creating duplicate messages or Groq API 400 errors (tool_use_failed).
    if (context.roomId) {
      META_TOOLS.add('collaboration/chat/send');
    }

    let prioritizedTools = toolDefinitions.filter(t => !META_TOOLS.has(t.name));

    // Three-tier prioritization with budget awareness
    const recipeToolNames = this.getRecipeToolNames(context);
    const hasRecipeTools = recipeToolNames.size > 0;
    const MAX_NATIVE_TOOLS = hasRecipeTools ? 32 : 64;

    if (prioritizedTools.length > MAX_NATIVE_TOOLS) {
      prioritizedTools = this.prioritizeTools(prioritizedTools, recipeToolNames, hasRecipeTools, MAX_NATIVE_TOOLS);
    }

    // Budget check: estimate token cost before converting (avoid repeated conversion)
    let tokenEstimate = this.estimateNativeToolTokens(convertToNativeToolSpecs(prioritizedTools));

    // If over budget, progressively drop lowest-priority tools
    while (tokenEstimate > allocatedBudget && prioritizedTools.length > 2) {
      prioritizedTools = prioritizedTools.slice(0, prioritizedTools.length - 3);
      tokenEstimate = this.estimateNativeToolTokens(convertToNativeToolSpecs(prioritizedTools));
    }

    // Convert once — final set
    const finalSpecs = convertToNativeToolSpecs(prioritizedTools);
    const finalTokens = tokenEstimate;

    // Native providers still need behavioral instruction IN the system prompt.
    // JSON tool specs alone don't tell the model to prefer action over prose.
    // Include contextual guidance based on what the user is asking about.
    const groupRegistry = ToolGroupRegistry.sharedInstance();
    const triggerText = context.options.currentMessage?.content || '';
    const selectedGroups = groupRegistry.selectGroups(triggerText, 3);
    const groupHints = selectedGroups
      .filter(g => !g.alwaysInclude)
      .map(g => `For ${g.label.toLowerCase()}: use ${g.toolPatterns.slice(0, 2).join(', ')}`)
      .join('. ');

    const nativeBehavioralNudge = `You MUST use tools to take action. Do not describe what you would do — DO IT. Prefer tool calls over prose.${groupHints ? `\n${groupHints}.` : ''}`;
    const nudgeTokens = this.estimateTokens(nativeBehavioralNudge);

    log.debug(`Native tools: ${finalSpecs.length} specs (~${finalTokens + nudgeTokens} tokens) for persona ${context.personaId.slice(0, 8)}`);

    return {
      sourceName: this.name,
      tokenCount: finalTokens + nudgeTokens,
      loadTimeMs: performance.now() - startTime,
      systemPromptSection: nativeBehavioralNudge,
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
   * Uses contextual tool group selection: analyzes the trigger message to determine
   * which tool GROUPS are relevant, then presents only those tools with few-shot
   * examples. A code question gets code tools with a code example. A task request
   * gets sentinel tools with a sentinel example.
   *
   * This dramatically reduces tool count (5-12 instead of 30-60) and gives models
   * concrete examples of HOW to call tools, not just WHAT tools exist.
   */
  private loadXmlTools(
    context: RAGSourceContext,
    toolDefinitions: ToolDefinition[],
    allocatedBudget: number,
    startTime: number
  ): RAGSection {
    // Exclude chat/send when responding in a chat room (same as native path)
    if (context.roomId) {
      toolDefinitions = toolDefinitions.filter(t => t.name !== 'collaboration/chat/send');
    }

    // Contextual group selection: analyze trigger message to find relevant tool groups
    const groupRegistry = ToolGroupRegistry.sharedInstance();
    const triggerText = context.options.currentMessage?.content || '';
    const recipeToolNames = this.getRecipeToolNames(context);

    // Select relevant groups based on what the user is asking about
    const selectedGroups = groupRegistry.selectGroups(triggerText, 5);

    // Filter tools to only those in selected groups + recipe tools
    let contextualTools = groupRegistry.filterToolsByGroups(toolDefinitions, selectedGroups);

    // Always include recipe tools at the top
    if (recipeToolNames.size > 0) {
      const recipeTools = toolDefinitions.filter(t => recipeToolNames.has(t.name));
      const contextualNames = new Set(contextualTools.map(t => t.name));
      for (const rt of recipeTools) {
        if (!contextualNames.has(rt.name)) {
          contextualTools.unshift(rt);
        }
      }
    }

    // Build grouped prompt with examples (shows tools organized by purpose)
    const groupedPrompt = groupRegistry.buildGroupedPrompt(selectedGroups, contextualTools);

    // Also format the actual tool specs for XML tool calling
    const adapter = getPrimaryAdapter();
    const formattedTools = adapter.formatToolsForPrompt(contextualTools);

    const behavioralNudge = `You MUST use tools to take action. Do not describe what you would do — DO IT.
When you see code-related questions: call code/read or code/search IMMEDIATELY.
When asked to fix or change something: call code/read first, then code/edit.
For complex tasks: call sentinel/coding-agent.

RESPOND WITH TOOL CALLS, NOT DESCRIPTIONS.`;

    const toolsSection = `\n\n=== YOUR CAPABILITIES ===\n${behavioralNudge}\n\n${groupedPrompt}\n\n=== TOOL DEFINITIONS ===\n${formattedTools}\n================================`;

    let finalSection = toolsSection;
    let tokenCount = this.estimateTokens(toolsSection);

    // Budget truncation: progressively remove tools from the end
    let reducedTools = contextualTools;
    if (tokenCount > allocatedBudget && reducedTools.length > 2) {
      while (tokenCount > allocatedBudget && reducedTools.length > 2) {
        const dropCount = reducedTools.length > 8 ? 3 : 1;
        reducedTools = reducedTools.slice(0, reducedTools.length - dropCount);
        const reduced = adapter.formatToolsForPrompt(reducedTools);
        // Keep grouped prompt (examples) even when truncating tool specs
        finalSection = `\n\n=== YOUR CAPABILITIES ===\n${behavioralNudge}\n\n${groupedPrompt}\n\n=== TOOL DEFINITIONS ===\n${reduced}\n================================`;
        tokenCount = this.estimateTokens(finalSection);
      }
    }

    const finalToolCount = reducedTools.length;
    const groupIds = selectedGroups.map(g => g.id).join(', ');
    log.debug(`XML tools: ${finalToolCount}/${toolDefinitions.length} tools in groups [${groupIds}] (~${tokenCount} tokens, budget=${allocatedBudget})`);

    return {
      sourceName: this.name,
      tokenCount,
      loadTimeMs: performance.now() - startTime,
      systemPromptSection: finalSection,
      metadata: {
        toolCount: finalToolCount,
        totalAvailable: toolDefinitions.length,
        format: 'xml',
        selectedGroups: groupIds,
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
    // Note: chat/send is excluded from tool list when responding in chat (see loadNativeTools/loadXmlTools)
    // so it won't appear here in chat context. chat/export replaces chat/history for reading conversation.
    const CRITICAL_TOOLS = new Set([
      'collaboration/chat/send', 'collaboration/chat/export',
    ]);

    // Essential prefix ordering — most important first.
    // When budget truncation drops from the end, ai/* goes first, then data/*, etc.
    const ESSENTIAL_PREFIX_ORDER: string[] = hasRecipeTools
      ? [] // When recipe tools exist, only allow exact essential matches
      : [
          'collaboration/chat/',     // Communication is #1
          'code/',                   // Code abilities are #2
          'sentinel/',              // Autonomous coding (Claude Code) #3
          'collaboration/decision/', // Decision making #4
          'collaboration/wall/',     // Shared documents #5
          'data/',                   // Data access #6
          'ai/',                     // AI meta-tools #7 (least important essential)
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
    // Recipe tools are on the build options (typed as RecipeToolDeclaration[])
    const recipeTools: RecipeToolDeclaration[] | undefined = context.options.recipeTools;
    if (!recipeTools || recipeTools.length === 0) {
      return new Set();
    }
    return new Set(
      recipeTools
        .filter((t) => t.enabledFor?.includes('ai'))
        .map((t) => t.name)
    );
  }

  /**
   * Estimate token count for native tool specs without serializing.
   * Avoids JSON.stringify on 50+ tool specs on every RAG build.
   * Approximation: name + description + parameter schema ≈ actual JSON overhead.
   */
  private estimateNativeToolTokens(specs: NativeToolSpec[]): number {
    let chars = 2; // array brackets
    for (const spec of specs) {
      // ~30 chars JSON overhead per tool + name + description + rough schema size
      chars += 30 + (spec.name?.length || 0) + (spec.description?.length || 0);
      if (spec.input_schema) {
        const schema = spec.input_schema;
        // Properties contribute ~50 chars each (key + type + description)
        const props = schema.properties;
        if (props) {
          const propKeys = Object.keys(props);
          for (const key of propKeys) {
            const prop = props[key] as Record<string, unknown> | undefined;
            const desc = typeof prop?.description === 'string' ? prop.description : '';
            chars += 50 + key.length + desc.length;
          }
        }
      }
    }
    return Math.ceil(chars / 4);
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
