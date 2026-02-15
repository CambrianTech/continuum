/**
 * AI Agent Command - Server Implementation
 * =========================================
 *
 * Universal agentic loop extracted from PersonaResponseGenerator.
 * Generates text, parses tool calls, executes tools, feeds results back,
 * and re-generates until the model stops calling tools.
 *
 * Model-adaptive:
 * - Safety caps tiered by provider (25/10/5)
 * - Tool format: native JSON (Anthropic/OpenAI) or XML (DeepSeek/local)
 * - Context window aware via AICapabilityRegistry
 *
 * Used by:
 * - Sentinel pipelines (LLM step with agentMode=true, via CommandExecutor IPC)
 * - Direct invocation (./jtag ai/agent --prompt="..." --tools='["code/tree"]')
 */

import { AiAgentCommand } from '../shared/AiAgentCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { AiAgentParams, AiAgentResult, ToolCallRecord } from '../shared/AiAgentTypes';
import { createAiAgentResult } from '../shared/AiAgentTypes';
import { AIProviderDaemon } from '../../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import type {
  TextGenerationRequest,
  ChatMessage,
  ContentPart,
  NativeToolSpec,
  ToolCall as NativeToolCall,
  ToolResult as NativeToolResult,
} from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import { AgentToolExecutor } from '../../../../system/tools/server/AgentToolExecutor';
import type { ToolCallContext } from '../../../../system/tools/server/AgentToolExecutor';
import {
  getAllToolDefinitionsAsync,
  type ToolAccessLevel,
} from '../../../../system/user/server/modules/PersonaToolDefinitions';
import {
  convertToNativeToolSpecs,
  supportsNativeTools,
  getToolCapability,
  getPrimaryAdapter,
  type ToolDefinition as AdapterToolDefinition,
} from '../../../../system/user/server/modules/ToolFormatAdapter';
import { generateUUID } from '../../../../system/core/types/CrossPlatformUUID';
import { LOCAL_MODELS } from '../../../../system/shared/Constants';

/** Default safety caps by provider tier */
function getSafetyMax(provider: string): number {
  if (['anthropic', 'openai', 'azure'].includes(provider)) return 25;
  if (supportsNativeTools(provider)) return 10;
  return 5;
}

export class AiAgentServerCommand extends AiAgentCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  async execute(params: AiAgentParams): Promise<AiAgentResult> {
    const start = Date.now();
    const allToolCallRecords: ToolCallRecord[] = [];

    try {
      // ── 1. Build messages ────────────────────────────────────────
      const messages: ChatMessage[] = [];

      if (params.systemPrompt) {
        messages.push({ role: 'system', content: params.systemPrompt });
      }

      if (params.messages && params.messages.length > 0) {
        messages.push(...params.messages);
      } else if (params.prompt) {
        messages.push({ role: 'user', content: params.prompt });
      } else {
        return createAiAgentResult(params, {
          success: false,
          error: 'Either prompt or messages is required',
        });
      }

      // ── 2. Resolve model + provider ──────────────────────────────
      const provider = params.provider || 'anthropic';
      const model = params.model || (
        provider === 'anthropic' ? 'claude-sonnet-4-5-20250929' :
        provider === 'candle' || provider === 'ollama' ? LOCAL_MODELS.DEFAULT :
        'claude-sonnet-4-5-20250929'
      );

      // ── 3. Resolve tools ─────────────────────────────────────────
      const toolCap = getToolCapability(provider);
      const useNative = supportsNativeTools(provider);

      let toolDefinitions: AdapterToolDefinition[] = [];
      let nativeToolSpecs: NativeToolSpec[] | undefined;

      if (toolCap !== 'none') {
        // Get all public tool definitions
        const allDefs = await getAllToolDefinitionsAsync('public' as ToolAccessLevel);

        // Filter to subset if specified
        if (params.tools !== undefined) {
          if (params.tools.length === 0) {
            // Explicit empty = no tools
            toolDefinitions = [];
          } else {
            const toolSet = new Set(params.tools);
            toolDefinitions = allDefs
              .filter(t => toolSet.has(t.name))
              .map(t => ({
                name: t.name,
                description: t.description,
                parameters: t.parameters,
                category: t.category,
              }));
          }
        } else {
          // undefined = all public tools
          toolDefinitions = allDefs.map(t => ({
            name: t.name,
            description: t.description,
            parameters: t.parameters,
            category: t.category,
          }));
        }

        // Convert to native format if provider supports it
        if (useNative && toolDefinitions.length > 0) {
          nativeToolSpecs = convertToNativeToolSpecs(toolDefinitions);
        }

        // For XML providers, inject tool docs into system prompt
        if (!useNative && toolDefinitions.length > 0) {
          const adapter = getPrimaryAdapter();
          const toolDocs = adapter.formatToolsForPrompt(toolDefinitions);
          // Prepend to first system message or create one
          const sysIdx = messages.findIndex(m => m.role === 'system');
          if (sysIdx >= 0) {
            const existing = typeof messages[sysIdx].content === 'string'
              ? messages[sysIdx].content as string
              : '';
            messages[sysIdx] = {
              ...messages[sysIdx],
              content: existing + '\n\n' + toolDocs,
            };
          } else {
            messages.unshift({ role: 'system', content: toolDocs });
          }
        }
      }

      // ── 4. Build generation request ──────────────────────────────
      const request: TextGenerationRequest = {
        messages,
        model,
        temperature: params.temperature ?? 0.7,
        maxTokens: params.maxTokens ?? 4096,
        preferredProvider: provider,
        tools: nativeToolSpecs,
        tool_choice: nativeToolSpecs && nativeToolSpecs.length > 0 ? 'auto' : undefined,
      };

      // ── 5. Initial generation ────────────────────────────────────
      let response = await AIProviderDaemon.generateText(request);

      // ── 6. Agentic tool loop ─────────────────────────────────────
      const safetyMax = params.maxIterations ?? getSafetyMax(provider);
      let iterations = 0;
      const executor = new AgentToolExecutor();

      // Build execution context for tool calls
      // callerId: sentinel handle or explicit caller, sessionId for session scope,
      // contextId: generated per-invocation (no persistent room/conversation scope)
      const callCtx: ToolCallContext = {
        callerId: params.sentinelHandle ?? params.callerId ?? params.sessionId ?? generateUUID(),
        sessionId: params.sessionId ?? generateUUID(),
        contextId: generateUUID(),
        context: params.context,
      };

      while (iterations < safetyMax) {
        // Check for tool calls (native first, then XML fallback)
        const hasNative = response.toolCalls && response.toolCalls.length > 0;
        const hasXml = !hasNative && toolCap === 'xml' && executor.parseToolCalls(response.text).length > 0;

        if (!hasNative && !hasXml) {
          // Model chose to stop — no more tool calls
          break;
        }

        iterations++;

        if (useNative && hasNative) {
          // ── Native tool protocol ──────────────────────────────
          const nativeCalls = response.toolCalls!;

          // Execute tools
          const toolStart = Date.now();
          const batchResult = await executor.executeNativeToolCalls(nativeCalls, callCtx);

          // Record tool calls
          for (let i = 0; i < nativeCalls.length; i++) {
            const nc = nativeCalls[i];
            const nr = batchResult.results[i];
            allToolCallRecords.push({
              toolName: nc.name,
              params: nc.input as Record<string, string>,
              success: !nr.is_error,
              content: !nr.is_error ? nr.content : undefined,
              error: nr.is_error ? nr.content : undefined,
              durationMs: Date.now() - toolStart,
            });
          }

          // Push assistant message with tool_use content blocks
          const assistantContent: ContentPart[] = response.content ?? [
            ...(response.text ? [{ type: 'text' as const, text: response.text }] : []),
            ...nativeCalls.map(tc => ({
              type: 'tool_use' as const,
              id: tc.id,
              name: tc.name,
              input: tc.input,
            })),
          ];
          messages.push({ role: 'assistant', content: assistantContent });

          // Push tool results as user message
          const toolResultContent: ContentPart[] = batchResult.results.map(r => ({
            type: 'tool_result' as const,
            tool_use_id: r.tool_use_id,
            content: r.content,
            ...(r.is_error && { is_error: true }),
          }));
          messages.push({ role: 'user', content: toolResultContent });

        } else {
          // ── XML fallback ──────────────────────────────────────
          const xmlCalls = executor.parseToolCalls(response.text);

          const toolStart = Date.now();
          const xmlResult = await executor.executeXmlToolCalls(xmlCalls, callCtx);

          for (const tc of xmlCalls) {
            allToolCallRecords.push({
              toolName: tc.toolName,
              params: tc.parameters,
              success: true, // XML batch doesn't report per-tool errors easily
              durationMs: Date.now() - toolStart,
            });
          }

          // Strip tool blocks from response for assistant message
          const explanationText = executor.stripToolBlocks(response.text);
          messages.push({ role: 'assistant', content: explanationText });

          // Full tool results as user message
          messages.push({ role: 'user', content: xmlResult.formattedResults });
        }

        // ── Regenerate ──────────────────────────────────────────
        const regenerated = await AIProviderDaemon.generateText({
          ...request,
          messages,
        });

        if (!regenerated.text && !regenerated.toolCalls?.length) {
          // Empty response — use previous text
          response.text = executor.stripToolBlocks(response.text);
          break;
        }

        response = regenerated;
      }

      // If we hit the safety cap, strip any remaining tool blocks
      if (iterations >= safetyMax) {
        response.text = executor.stripToolBlocks(response.text);
      }

      // ── 7. Return result ─────────────────────────────────────────
      return createAiAgentResult(params, {
        success: true,
        text: response.text?.trim() || '',
        toolCalls: allToolCallRecords,
        iterations,
        tokenUsage: response.usage ? {
          input: response.usage.inputTokens,
          output: response.usage.outputTokens,
        } : undefined,
        model: response.model,
        provider: response.provider,
        durationMs: Date.now() - start,
      });

    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      return createAiAgentResult(params, {
        success: false,
        text: '',
        toolCalls: allToolCallRecords,
        iterations: 0,
        error: errorMsg,
        durationMs: Date.now() - start,
      });
    }
  }
}
