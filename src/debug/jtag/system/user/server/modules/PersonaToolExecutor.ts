/**
 * PersonaToolExecutor - Handles tool calling for PersonaUser
 *
 * Wraps AgentToolExecutor (universal tool execution) with persona-specific
 * pre/post processing:
 * - Workspace auto-bootstrap for code/* tools
 * - Tool result storage as ChatMessageEntity (working memory)
 * - Media collection with persona config filtering
 * - Cognition telemetry logging
 * - Sentinel auto-config for shell commands
 *
 * KEY METHODS:
 * - executeSingleTool()       — core per-tool pipeline (delegate + persona pre/post)
 * - executeToolCalls()        — XML-formatted batch execution (for XML fallback path)
 * - executeNativeToolCalls()  — structured batch execution (for native tool_result protocol)
 */

import { CognitionLogger } from './cognition/CognitionLogger';
import { SentinelAutoConfig } from '../../../code/server/SentinelAutoConfig';
import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { generateUUID } from '../../../core/types/CrossPlatformUUID';
import type { MediaItem } from '../../../data/entities/ChatMessageEntity';
import { ChatMessageEntity } from '../../../data/entities/ChatMessageEntity';
import type { PersonaMediaConfig } from './PersonaMediaConfig';
import { unsanitizeToolName } from './ToolFormatAdapter';
import { Logger } from '../../../core/logging/Logger';
import { AgentToolExecutor, type ToolCall, type ToolCallContext } from '../../../tools/server/AgentToolExecutor';

import { DataCreate } from '../../../../commands/data/create/shared/DataCreateTypes';
import type {
  ToolCall as NativeToolCall,
  ToolResult as NativeToolResult,
} from '@daemons/ai-provider-daemon/shared/AIProviderTypesV2';

// Re-export ToolCall for backward compat (PersonaResponseGenerator imports from here)
export type { ToolCall };

/**
 * Tool execution context with media configuration
 */
export interface ToolExecutionContext {
  personaId: UUID;
  personaName: string;
  sessionId: UUID;  // AI's own sessionId for sandboxed tool execution
  contextId: UUID;
  context: import('../../../core/types/JTAGTypes').JTAGContext;  // PersonaUser's enriched context (with callerType='persona')
  personaConfig: PersonaMediaConfig;
}

/**
 * Tool execution result with structured media
 */
export interface ToolResult {
  toolName: string;
  success: boolean;
  content?: string;
  media?: MediaItem[];  // Structured media from tool execution
  error?: string;
}

/**
 * Result from executing a single tool through the full pipeline.
 * Used internally by executeToolCalls and executeNativeToolCalls.
 */
export interface SingleToolExecution {
  result: ToolResult;
  resultId: UUID;
  media: MediaItem[];
}

/**
 * Minimal persona info needed by PersonaToolExecutor
 */
export interface PersonaUserForToolExecutor {
  readonly id: UUID;
  readonly displayName: string;
  readonly homeDirectory: string;
  readonly entity: {
    readonly uniqueId: string;
  };
  /** Auto-bootstrap workspace when code/* tools are invoked. Called once per context. */
  readonly ensureCodeWorkspace?: () => Promise<void>;
}

export class PersonaToolExecutor {

  private readonly agentExecutor: AgentToolExecutor;
  private persona: PersonaUserForToolExecutor;
  private log: ReturnType<typeof Logger.create>;
  private workspaceBootstrapped = false;

  constructor(personaUser: PersonaUserForToolExecutor) {
    this.persona = personaUser;
    this.agentExecutor = new AgentToolExecutor();

    // Per-persona tools.log in their home directory
    const category = 'logs/tools';
    this.log = Logger.create(
      `PersonaToolExecutor:${this.persona.displayName}`,
      category,
      this.persona.homeDirectory
    );
  }

  /**
   * Parse tool calls from AI response text using registered format adapters.
   * Delegates to AgentToolExecutor.
   */
  parseToolCalls(responseText: string): ToolCall[] {
    return this.agentExecutor.parseToolCalls(responseText);
  }

  // ──────────────────────────────────────────────
  // Core Pipeline: Batch Preparation + Single Tool Execution
  // ──────────────────────────────────────────────

  /**
   * Prepare a batch of tool calls for execution.
   * Handles loop detection filtering and workspace auto-bootstrap.
   */
  private async prepareBatch(
    toolCalls: ToolCall[],
    context: ToolExecutionContext,
  ): Promise<ToolCall[]> {
    // Filter out looping tool calls before execution (delegate to AgentToolExecutor)
    const filtered = toolCalls.filter(toolCall => {
      if (this.agentExecutor.isLoopDetected(toolCall.toolName, toolCall.parameters, this.persona.id)) {
        this.log.warn(`Skipping looping tool call: ${toolCall.toolName}`);
        return false;
      }
      return true;
    });

    // Auto-bootstrap workspace if any code/* tools are being called
    const hasCodeTools = filtered.some(tc => tc.toolName.startsWith('code/'));
    this.log.debug(`prepareBatch: ${filtered.length} tools, hasCodeTools=${hasCodeTools}, bootstrapped=${this.workspaceBootstrapped}`);

    if (hasCodeTools && !this.workspaceBootstrapped) {
      if (this.persona.ensureCodeWorkspace) {
        try {
          this.log.info('Auto-bootstrapping workspace for code/* tool execution');
          await this.persona.ensureCodeWorkspace();
          this.workspaceBootstrapped = true;
          this.log.info('Workspace bootstrapped successfully');
        } catch (err: any) {
          this.log.error(`Failed to bootstrap workspace: ${err.message}`);
        }
      } else {
        this.log.warn('code/* tools called but ensureCodeWorkspace callback not available');
      }
    }

    return filtered;
  }

  /**
   * Execute a single tool call through the full persona pipeline.
   *
   * 1. Delegate correction + execution to AgentToolExecutor
   * 2. Persona post-processing: sentinel auto-config, logging, storage, media collection
   */
  private async executeSingleTool(
    toolCall: ToolCall,
    context: ToolExecutionContext,
  ): Promise<SingleToolExecution> {
    const startTime = Date.now();

    // Apply name/param corrections via AgentToolExecutor
    const { corrected, nameChanged, paramsChanged } = this.agentExecutor.correctToolCall(toolCall);
    if (nameChanged) {
      this.log.info(`Redirected ${toolCall.toolName} -> ${corrected.toolName}`);
    }
    for (const change of paramsChanged) {
      this.log.info(`Param corrected: ${change}`);
    }

    // Build ToolCallContext from persona context
    const callCtx: ToolCallContext = {
      callerId: context.personaId,
      sessionId: context.sessionId,
      contextId: context.contextId,
      context: context.context,
    };

    // Log tool call
    const paramsJson = JSON.stringify(corrected.parameters, null, 2);
    this.log.info(`CALL: ${corrected.toolName}`);
    this.log.info(`  params: ${paramsJson.replace(/\n/g, '\n  ')}`);

    // Execute via AgentToolExecutor (corrections already applied, so pass corrected directly)
    const agentResult = await this.agentExecutor.executeToolCall(
      corrected.toolName,
      corrected.parameters,
      callCtx
    );

    // Build ToolResult with media from registry result
    // NOTE: AgentToolExecutor.executeToolCall returns ToolCallResult without media.
    // For media support, we need to check if the underlying ToolRegistry result had media.
    // The ToolRegistry.executeTool is called internally by AgentToolExecutor, which strips media.
    // For personas that need media (screenshots), we re-query or accept no-media here.
    // The media is still available through the tool result content (base64 in JSON).
    const result: ToolResult = {
      toolName: agentResult.toolName,
      success: agentResult.success,
      content: agentResult.success ? agentResult.content : undefined,
      error: agentResult.error,
      // Media comes through the content JSON for now — tools like screenshot
      // embed base64 in the result content which PersonaResponseGenerator handles
    };

    // Auto-inject sentinel rules for code/shell/execute commands (fire-and-forget)
    if (corrected.toolName === 'code/shell/execute' && result.success && result.content) {
      try {
        const execResult = JSON.parse(result.content);
        if (execResult.executionId && execResult.status === 'running') {
          SentinelAutoConfig.applyIfApplicable(
            context.personaId,
            execResult.executionId,
            corrected.parameters.cmd || '',
          ).catch(err => this.log.warn(`Sentinel auto-config failed: ${err.message}`));
        }
      } catch {
        // Result wasn't JSON or missing fields — skip sentinel
      }
    }

    const duration = Date.now() - startTime;

    // Log result
    if (result.success) {
      let resultSummary = result.content?.slice(0, 500) || 'no content';
      try {
        const parsed = JSON.parse(result.content || '');
        const keyFields = ['success', 'message', 'newMode', 'previousMode', 'count', 'items', 'data'];
        const summary: Record<string, unknown> = {};
        for (const key of keyFields) {
          if (parsed[key] !== undefined) {
            summary[key] = Array.isArray(parsed[key]) ? `[${parsed[key].length} items]` : parsed[key];
          }
        }
        if (Object.keys(summary).length > 0) {
          resultSummary = JSON.stringify(summary);
        }
      } catch { /* not JSON, use raw */ }

      this.log.info(`RESULT: OK ${duration}ms`);
      this.log.info(`  ${resultSummary}${result.content && result.content.length > 500 ? '...' : ''}`);
    } else {
      this.log.error(`RESULT: FAIL ${duration}ms`);
      this.log.error(`  error: ${result.error || 'unknown error'}`);
    }

    // Store tool result in working memory
    const resultId = await this.storeToolResult(
      corrected.toolName,
      corrected.parameters,
      {
        success: result.success,
        data: result.content,
        error: result.error,
      },
      context.contextId
    );
    this.log.debug(`Stored tool result #${resultId.slice(0, 8)}`);

    // Collect media for this tool (persona config filtering)
    const collectedMedia: MediaItem[] = [];
    const isScreenshotTool = corrected.toolName === 'screenshot' || corrected.toolName === 'interface/screenshot';

    // Try to extract media from result content (tools embed media in JSON)
    if (result.content) {
      try {
        const parsed = JSON.parse(result.content);
        if (parsed.media) {
          const mediaItems: MediaItem[] = Array.isArray(parsed.media) ? parsed.media : [parsed.media];
          const shouldLoadMedia = context.personaConfig.autoLoadMedia || isScreenshotTool;

          if (shouldLoadMedia && mediaItems.length > 0) {
            const supportedMedia = mediaItems.filter(m =>
              isScreenshotTool || context.personaConfig.supportedMediaTypes.includes(m.type)
            );
            if (supportedMedia.length > 0) {
              this.log.info(`Loading ${supportedMedia.length} media (types: ${supportedMedia.map(m => m.type).join(', ')})${isScreenshotTool ? ' [screenshot override]' : ''}`);
              collectedMedia.push(...supportedMedia);
            }
          }
        }
      } catch {
        // Not JSON or no media field
      }
    }

    // Fire-and-forget: Cognition telemetry
    CognitionLogger.logToolExecution(
      this.persona.id,
      this.persona.displayName,
      corrected.toolName,
      corrected.parameters,
      result.success ? 'success' : 'error',
      duration,
      'chat',
      context.contextId,
      {
        toolResult: result.content?.slice(0, 1000),
        errorMessage: result.error,
        storedResultId: resultId
      }
    ).catch(err => this.log.error('Failed to log tool execution:', err));

    return { result, resultId, media: collectedMedia };
  }

  // ──────────────────────────────────────────────
  // Public API: Batch Tool Execution
  // ──────────────────────────────────────────────

  /**
   * Execute tool calls and return XML-formatted results + optional media.
   * Used by the XML fallback path for non-native providers.
   */
  async executeToolCalls(
    toolCalls: ToolCall[],
    context: ToolExecutionContext
  ): Promise<{
    formattedResults: string;
    media?: MediaItem[];
    storedResultIds: UUID[];
  }> {
    if (toolCalls.length === 0) {
      return { formattedResults: '', storedResultIds: [] };
    }

    this.log.info(`Executing ${toolCalls.length} tool(s): ${toolCalls.map(t => t.toolName).join(', ')}`);

    const filtered = await this.prepareBatch(toolCalls, context);
    if (filtered.length === 0) {
      this.log.warn('All tool calls blocked by loop detection');
      return { formattedResults: '[All tool calls blocked - infinite loop detected]', storedResultIds: [] };
    }

    // Execute all tools concurrently
    const executions = await Promise.all(filtered.map(tc => this.executeSingleTool(tc, context)));

    const allMedia = executions.flatMap(e => e.media);
    const storedResultIds = executions.map(e => e.resultId);
    const successCount = executions.filter(e => e.result.success).length;
    this.log.info(`Complete: ${successCount}/${toolCalls.length} successful, ${allMedia.length} media loaded, ${storedResultIds.length} stored`);

    return {
      formattedResults: executions.map(e => this.formatToolResult(e.result)).join('\n\n'),
      media: allMedia.length > 0 ? allMedia : undefined,
      storedResultIds,
    };
  }

  /**
   * Execute native tool calls from the canonical agent loop.
   * Returns per-tool ToolResult objects with full content and tool_use_id correlation.
   */
  async executeNativeToolCalls(
    nativeToolCalls: NativeToolCall[],
    context: ToolExecutionContext,
    maxResultChars = 30_000,
  ): Promise<{
    results: NativeToolResult[];
    media: MediaItem[];
    storedIds: UUID[];
  }> {
    if (nativeToolCalls.length === 0) {
      return { results: [], media: [], storedIds: [] };
    }

    // Convert native format → executor format
    const executorCalls: ToolCall[] = nativeToolCalls.map(tc => ({
      toolName: unsanitizeToolName(tc.name),
      parameters: Object.fromEntries(
        Object.entries(tc.input).map(([k, v]) => [k, String(v)])
      ) as Record<string, string>,
    }));

    // Prepare batch (loop detection + workspace bootstrap)
    const filtered = await this.prepareBatch(executorCalls, context);

    // Execute filtered tools in parallel
    const executions = await Promise.all(filtered.map(tc => this.executeSingleTool(tc, context)));

    // Map results back to native tool calls with tool_use_id correlation
    const filteredSet = new Set(filtered);
    const results: NativeToolResult[] = [];
    let execIdx = 0;

    for (let i = 0; i < nativeToolCalls.length; i++) {
      if (!filteredSet.has(executorCalls[i])) {
        results.push({
          tool_use_id: nativeToolCalls[i].id,
          content: 'Tool call blocked by loop detection.',
          is_error: true,
        });
        continue;
      }

      const exec = executions[execIdx++];
      let content = exec.result.success
        ? (exec.result.content || 'No content returned')
        : (exec.result.error || 'Unknown error');

      if (content.length > maxResultChars) {
        content = content.slice(0, maxResultChars) + `\n[...truncated, ${content.length} chars total]`;
      }

      results.push({
        tool_use_id: nativeToolCalls[i].id,
        content,
        is_error: !exec.result.success || undefined,
      });
    }

    return {
      results,
      media: executions.flatMap(e => e.media),
      storedIds: executions.map(e => e.resultId),
    };
  }

  /**
   * Format tool result as XML
   */
  private formatToolResult(result: ToolResult): string {
    if (result.success && result.content) {
      return `<tool_result>
<tool_name>${result.toolName}</tool_name>
<status>success</status>
<content>
${result.content}
</content>
</tool_result>`;
    } else {
      return `<tool_result>
<tool_name>${result.toolName}</tool_name>
<status>error</status>
<error>
\`\`\`
${result.error || 'Unknown error'}
\`\`\`
</error>
</tool_result>`;
    }
  }

  /**
   * Strip tool blocks from response text to get clean user-facing message.
   * Delegates to AgentToolExecutor.
   */
  stripToolBlocks(responseText: string): string {
    return this.agentExecutor.stripToolBlocks(responseText);
  }

  /**
   * Get list of available tools (from ToolRegistry)
   */
  getAvailableTools(): string[] {
    return this.agentExecutor.availableTools;
  }

  /**
   * Store tool result as ChatMessageEntity for working memory
   */
  async storeToolResult(
    toolName: string,
    parameters: Record<string, unknown>,
    result: { success: boolean; data: unknown; error?: string; media?: MediaItem[] },
    roomId: UUID
  ): Promise<UUID> {
    const summary = this.generateSummary(toolName, result);

    const message = new ChatMessageEntity();
    message.id = generateUUID();
    message.roomId = roomId;
    message.senderId = this.persona.id;
    message.senderName = this.persona.displayName;
    message.senderType = 'system';
    message.content = { text: summary, media: result.media || [] };
    message.metadata = {
      toolResult: true,
      toolName,
      parameters,
      fullData: result.data,
      success: result.success,
      error: result.error,
      storedAt: Date.now()
    };
    message.timestamp = new Date();
    message.status = 'sent';
    message.priority = 'normal';
    message.reactions = [];

    await DataCreate.execute<ChatMessageEntity>({
      collection: ChatMessageEntity.collection,
      backend: 'server',
      data: message
    });

    this.log.debug(`Stored tool result #${message.id.slice(0, 8)} (${summary})`);
    return message.id;
  }

  /**
   * Generate short summary of tool result for RAG context
   */
  private generateSummary(
    toolName: string,
    result: { success: boolean; data: unknown; error?: unknown }
  ): string {
    if (!result.success) {
      return `Tool '${toolName}' failed: ${this.stringifyError(result.error)}`;
    }

    const data = result.data;
    const action = toolName.split('/').pop() ?? toolName;

    if (Array.isArray(data)) {
      return `${action}: ${data.length} item${data.length !== 1 ? 's' : ''}`;
    }

    if (typeof data === 'string') {
      const lines = data.split('\n').filter(l => l.trim()).length;
      return lines > 1 ? `${action}: ${lines} lines` : `${action}: ${data.slice(0, 120)}`;
    }

    if (data && typeof data === 'object') {
      const obj = data as Record<string, unknown>;
      const parts: string[] = [];

      const filePath = obj.filePath ?? obj.file_path ?? obj.path ?? obj.fileName ?? obj.file_name;
      if (filePath) parts.push(String(filePath));

      const bytes = obj.bytesWritten ?? obj.bytes_written ?? obj.size ?? obj.byteLength;
      if (typeof bytes === 'number') parts.push(`${bytes} bytes`);

      const count = obj.count ?? obj.total ?? obj.matches ?? obj.length;
      if (typeof count === 'number') parts.push(`${count} items`);

      const width = obj.width;
      const height = obj.height;
      if (typeof width === 'number' && typeof height === 'number') parts.push(`${width}x${height}`);

      if (parts.length > 0) return `${action}: ${parts.join(', ')}`;
    }

    const dataStr = typeof data === 'string' ? data : JSON.stringify(data);
    return `${action}: ${dataStr.slice(0, 120)}${dataStr.length > 120 ? '...' : ''}`;
  }

  /**
   * Convert any error value to a human-readable string
   */
  private stringifyError(error: unknown): string {
    if (error === undefined || error === null) return 'Unknown error';
    if (typeof error === 'string') return error;
    if (error instanceof Error) return error.message;

    if (typeof error === 'object' && error !== null) {
      const obj = error as Record<string, unknown>;
      if (typeof obj.message === 'string') return obj.message;
      if (typeof obj.error === 'string') return obj.error;
      if (typeof obj.errorMessage === 'string') return obj.errorMessage;
      if (typeof obj.msg === 'string') return obj.msg;

      if (obj.error && typeof obj.error === 'object') {
        const nested = obj.error as Record<string, unknown>;
        if (typeof nested.message === 'string') return nested.message;
      }

      try {
        const str = JSON.stringify(error);
        return str.length > 500 ? `${str.slice(0, 500)}...` : str;
      } catch {
        return 'Error object could not be serialized';
      }
    }

    return String(error);
  }
}
