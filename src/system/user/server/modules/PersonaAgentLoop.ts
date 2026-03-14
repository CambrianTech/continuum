/**
 * PersonaAgentLoop — Tool execution loop for AI response generation
 *
 * Extracted from PersonaResponseGenerator. Handles the canonical agent loop:
 * while model returns tool_use → execute tools → feed results → regenerate.
 *
 * The model decides when to stop (finishReason !== 'tool_use').
 * Safety cap prevents infinite loops for less capable models.
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { MediaItem } from '../../../data/entities/ChatMessageEntity';
import { AIProviderDaemon } from '../../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import type {
  TextGenerationRequest,
  TextGenerationResponse,
  ChatMessage,
  ContentPart,
  NativeToolSpec,
  ToolCall as NativeToolCall,
  ToolResult as NativeToolResult,
} from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';
import type { PersonaToolExecutor } from './PersonaToolExecutor';
import type { PersonaMediaConfig } from './PersonaMediaConfig';
import type { PersonaResponseValidator } from './PersonaResponseValidator';
import type { PersonaPromptAssembler } from './PersonaPromptAssembler';
import { supportsNativeTools, sanitizeToolName, coerceParamsToSchema } from './ToolFormatAdapter';
import type { JTAGContext } from '../../../core/types/JTAGTypes';
import { Events } from '../../../core/shared/Events';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { PRESENCE_EVENTS } from '../../../core/shared/EventConstants';

export interface AgentLoopContext {
  personaId: UUID;
  personaName: string;
  provider: string;
  roomId: UUID;
  sessionId: UUID;
  context: JTAGContext;
  toolExecutor: PersonaToolExecutor;
  responseValidator: PersonaResponseValidator;
  promptAssembler: PersonaPromptAssembler;
  mediaConfig: PersonaMediaConfig;
  log: (message: string, ...args: unknown[]) => void;
}

export interface AgentLoopResult {
  toolIterations: number;
  durationMs: number;
  storedToolResultIds: UUID[];
}

/**
 * Safety cap for agent tool loop iterations, tiered by model capability.
 * Frontier models (Anthropic, OpenAI) are trusted to self-terminate via finishReason.
 * Mid-tier models with native tool support get moderate cap.
 * XML-based / local models get tight leash since they can't signal "I'm done" via finishReason.
 */
function getSafetyMaxIterations(provider: string): number {
  if (['anthropic', 'openai', 'azure'].includes(provider)) return 25;
  if (supportsNativeTools(provider)) return 10;
  return 5;
}

/**
 * Run the canonical agent tool loop.
 *
 * Mutates `aiResponse` in place (text, toolCalls, content, finishReason).
 * Appends tool call/result messages to `messages` array.
 */
export async function runAgentLoop(
  ctx: AgentLoopContext,
  messages: ChatMessage[],
  request: TextGenerationRequest,
  aiResponse: TextGenerationResponse,
): Promise<AgentLoopResult> {
  const agentLoopStart = Date.now();
  const SAFETY_MAX = getSafetyMaxIterations(ctx.provider);
  let toolIterations = 0;
  const useNativeProtocol = supportsNativeTools(ctx.provider);
  const allStoredResultIds: UUID[] = [];

  // Build execution context once (loop-invariant)
  const enrichedContext = { ...ctx.context, userId: ctx.personaId };
  const toolExecutionContext = {
    personaId: ctx.personaId,
    personaName: ctx.personaName,
    sessionId: ctx.sessionId,
    contextId: ctx.roomId,
    context: enrichedContext,
    personaConfig: ctx.mediaConfig,
  };

  while (toolIterations < SAFETY_MAX) {
    // Check for tool calls — native first, then XML fallback
    const hasNativeToolCalls = aiResponse.toolCalls && aiResponse.toolCalls.length > 0;
    const parsed = !hasNativeToolCalls ? await ctx.toolExecutor.parseResponse(aiResponse.text) : null;
    const hasXmlToolCalls = parsed !== null && parsed.toolCalls.length > 0;

    if (!hasNativeToolCalls && !hasXmlToolCalls) {
      if (toolIterations > 0) {
        ctx.log(`✅ ${ctx.personaName}: [AGENT-LOOP] Model stopped after ${toolIterations} iteration(s)`);
      }
      break;
    }

    toolIterations++;
    ctx.log(`🔧 ${ctx.personaName}: [AGENT-LOOP] Iteration ${toolIterations}/${SAFETY_MAX}`);

    // Refresh typing indicator during tool loop (3s decay timer would otherwise expire)
    if (DataDaemon.jtagContext) {
      Events.emit(DataDaemon.jtagContext, PRESENCE_EVENTS.TYPING_START, {
        userId: ctx.personaId, displayName: ctx.personaName, roomId: ctx.roomId
      }).catch(() => {});
    }

    if (hasNativeToolCalls || (useNativeProtocol && hasXmlToolCalls)) {
      // ── Native tool protocol (Anthropic, OpenAI, Groq, Together, etc.) ──
      let nativeToolCalls: NativeToolCall[];
      if (hasNativeToolCalls) {
        nativeToolCalls = aiResponse.toolCalls!;
      } else {
        // Synthesize native format from text-parsed calls
        const toolSpecs = (request.tools as NativeToolSpec[]) ?? [];
        nativeToolCalls = parsed!.toolCalls.map((tc, i) => {
          const name = sanitizeToolName(tc.toolName);
          return {
            id: `synth_${Date.now()}_${i}`,
            name,
            input: coerceParamsToSchema(tc.parameters ?? {}, toolSpecs, name),
          };
        });
      }
      ctx.log(`🔧 ${ctx.personaName}: [AGENT-LOOP] Executing ${nativeToolCalls.length} native tool call(s)${!hasNativeToolCalls ? ' (synthesized from text)' : ''}`);

      let toolResults: NativeToolResult[];
      let toolMedia: MediaItem[] = [];
      try {
        const execResult = await ctx.toolExecutor.executeNativeToolCalls(
          nativeToolCalls,
          toolExecutionContext,
        );
        toolResults = execResult.results;
        toolMedia = execResult.media;
        allStoredResultIds.push(...execResult.storedIds);
      } catch (toolExecError) {
        const errMsg = toolExecError instanceof Error ? toolExecError.message : String(toolExecError);
        ctx.log(`❌ ${ctx.personaName}: [AGENT-LOOP] Tool execution failed: ${errMsg}`);
        toolResults = nativeToolCalls.map(tc => ({
          toolUseId: tc.id,
          content: `Tool execution error: ${errMsg}`,
          isError: true as const,
        }));
      }

      // Push assistant message with tool_use content blocks
      const assistantContent: ContentPart[] = hasNativeToolCalls
        ? (aiResponse.content ?? [
            ...(aiResponse.text ? [{ type: 'text' as const, text: aiResponse.text }] : []),
            ...nativeToolCalls.map(tc => ({
              type: 'tool_use' as const,
              id: tc.id,
              name: tc.name,
              input: tc.input,
            })),
          ])
        : [
            ...(parsed!.cleanedText ? [{ type: 'text' as const, text: parsed!.cleanedText }] : []),
            ...nativeToolCalls.map(tc => ({
              type: 'tool_use' as const,
              id: tc.id,
              name: tc.name,
              input: tc.input,
            })),
          ];
      messages.push({ role: 'assistant' as const, content: assistantContent });

      // Push tool results as user message with tool_result content blocks (FULL results)
      const toolResultContent: ContentPart[] = toolResults.map(r => ({
        type: 'tool_result' as const,
        tool_use_id: r.toolUseId,
        content: r.content,
        is_error: r.isError ?? null,
      }));

      if (toolMedia.length > 0) {
        toolResultContent.push(...ctx.promptAssembler.mediaToContentParts(toolMedia));
      }

      messages.push({ role: 'user' as const, content: toolResultContent });

    } else if (hasXmlToolCalls) {
      // ── XML path for non-native providers (DeepSeek, Candle, local) ──
      const xmlToolCalls = parsed!.toolCalls;
      ctx.log(`🔧 ${ctx.personaName}: [AGENT-LOOP] Executing ${xmlToolCalls.length} XML tool call(s)`);

      let formattedResults: string;
      let xmlToolMedia: MediaItem[] = [];
      try {
        const xmlExecResult = await ctx.toolExecutor.executeToolCalls(
          xmlToolCalls,
          toolExecutionContext,
        );
        formattedResults = xmlExecResult.formattedResults;
        xmlToolMedia = xmlExecResult.media ?? [];
        allStoredResultIds.push(...xmlExecResult.storedResultIds);
      } catch (toolExecError) {
        const errMsg = toolExecError instanceof Error ? toolExecError.message : String(toolExecError);
        ctx.log(`❌ ${ctx.personaName}: [AGENT-LOOP] XML tool execution failed: ${errMsg}`);
        formattedResults = `<tool_result>\n<status>error</status>\n<error>\n\`\`\`\nTool execution error: ${errMsg}\n\`\`\`\n</error>\n</tool_result>`;
      }

      const explanationText = parsed!.cleanedText;
      messages.push({ role: 'assistant' as const, content: explanationText });

      const toolResultContent: (ContentPart | { type: 'text'; text: string })[] = [
        { type: 'text' as const, text: formattedResults },
      ];
      if (xmlToolMedia.length > 0) {
        toolResultContent.push(...ctx.promptAssembler.mediaToContentParts(xmlToolMedia));
      }
      messages.push({ role: 'user' as const, content: toolResultContent });
    }

    // Regenerate — force text response after 3 tool iterations.
    const forceText = toolIterations >= 3 || toolIterations >= SAFETY_MAX - 1;
    const regenerationTools = forceText ? undefined : request.tools;
    const regenerationToolChoice = forceText ? undefined : request.toolChoice;

    ctx.log(`🔧 ${ctx.personaName}: [AGENT-LOOP] Regenerating with ${messages.length} messages (tools ${forceText ? 'DISABLED — forcing text response' : 'enabled'})`);

    try {
      const regenerateStartTime = Date.now();
      const regeneratedResponse = await AIProviderDaemon.generateText({
        ...request,
        messages,
        tools: regenerationTools,
        toolChoice: regenerationToolChoice,
      });
      const regenerateDuration = Date.now() - regenerateStartTime;

      ctx.log(`⏱️  ${ctx.personaName}: [AGENT-LOOP] Regeneration took ${regenerateDuration}ms, finishReason: ${regeneratedResponse.finishReason}`);

      if (!regeneratedResponse.text && !regeneratedResponse.toolCalls?.length) {
        ctx.log(`⚠️  ${ctx.personaName}: [AGENT-LOOP] Empty response from ${ctx.provider} after ${toolIterations} tool iteration(s), using cleaned previous text`);
        const fallback = await ctx.toolExecutor.parseResponse(aiResponse.text);
        aiResponse.text = fallback.cleanedText;
        break;
      }

      // Update full response state — clean via validator
      const loopCleaned = await ctx.responseValidator.cleanResponse(regeneratedResponse.text?.trim() || '');
      if (loopCleaned.text.length > 0) {
        aiResponse.text = loopCleaned.text;
      } else if (regeneratedResponse.text?.trim()) {
        ctx.log(`⚠️ ${ctx.personaName}: [AGENT-LOOP] Regenerated response empty after cleaning — keeping previous text`);
      }
      aiResponse.toolCalls = regeneratedResponse.toolCalls ?? undefined;
      aiResponse.content = regeneratedResponse.content ?? undefined;
      aiResponse.finishReason = regeneratedResponse.finishReason;

      ctx.log(`✅ ${ctx.personaName}: [AGENT-LOOP] Got response (${aiResponse.text.length} chars, toolCalls: ${aiResponse.toolCalls?.length ?? 0})`);

      if (forceText) {
        ctx.log(`✅ ${ctx.personaName}: [AGENT-LOOP] Forced text response after ${toolIterations} iteration(s), stopping`);
        break;
      }
    } catch (regenerateError) {
      const errorMsg = regenerateError instanceof Error ? regenerateError.message : String(regenerateError);
      ctx.log(`❌ ${ctx.personaName}: [AGENT-LOOP] Regeneration failed: ${errorMsg}`);
      aiResponse.text = (await ctx.toolExecutor.parseResponse(aiResponse.text)).cleanedText;
      break;
    }
  }

  if (toolIterations >= SAFETY_MAX) {
    ctx.log(`⚠️  ${ctx.personaName}: [AGENT-LOOP] Hit safety cap (${SAFETY_MAX}), stopping`);
  }

  // Always strip any remaining tool call text from the final response
  if (toolIterations > 0 && aiResponse.text) {
    const finalCleaned = await ctx.toolExecutor.parseResponse(aiResponse.text);
    if (finalCleaned.toolCalls.length > 0) {
      ctx.log(`🧹 ${ctx.personaName}: [AGENT-LOOP] Stripped ${finalCleaned.toolCalls.length} residual tool call(s) from final response`);
      aiResponse.text = finalCleaned.cleanedText;
    }
  }

  return {
    toolIterations,
    durationMs: Date.now() - agentLoopStart,
    storedToolResultIds: allStoredResultIds,
  };
}
