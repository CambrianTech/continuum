/**
 * PersonaToolExecutor - Handles tool calling for PersonaUser
 *
 * Parses tool calls from AI responses, executes them via ToolRegistry,
 * and formats results for injection back into conversation.
 *
 * CLEAN ARCHITECTURE:
 * - Uses ToolRegistry for ALL command execution (no hardcoded handlers)
 * - XML parsing only (no command-specific logic)
 * - Logging and metrics
 *
 * KEY METHODS:
 * - executeSingleTool()       — core per-tool pipeline (corrections, execution, storage, media)
 * - executeToolCalls()        — XML-formatted batch execution (for XML fallback path)
 * - executeNativeToolCalls()  — structured batch execution (for native tool_result protocol)
 */

import { CognitionLogger } from './cognition/CognitionLogger';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { generateUUID } from '../../../core/types/CrossPlatformUUID';
import { ToolRegistry } from '../../../tools/server/ToolRegistry';
import type { MediaItem } from '../../../data/entities/ChatMessageEntity';
import { ChatMessageEntity } from '../../../data/entities/ChatMessageEntity';
import type { PersonaMediaConfig } from './PersonaMediaConfig';
import { getToolFormatAdapters, type ToolFormatAdapter } from './ToolFormatAdapter';
import { unsanitizeToolName } from './ToolFormatAdapter';
import { Logger } from '../../../core/logging/Logger';
import { RoomResolver } from '../../../core/server/RoomResolver';

import { DataCreate } from '../../../../commands/data/create/shared/DataCreateTypes';
import type {
  ToolCall as NativeToolCall,
  ToolResult as NativeToolResult,
} from '@daemons/ai-provider-daemon/shared/AIProviderTypesV2';
/**
 * Parsed tool call from AI response
 */
export interface ToolCall {
  toolName: string;
  parameters: Record<string, string>;
}

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

  /**
   * Tool name corrections: LLMs sometimes confuse similarly-named tools.
   * workspace/tree shows the JTAG command hierarchy, code/tree shows workspace files.
   */
  private static readonly TOOL_CORRECTIONS: Record<string, string> = {
    'workspace/tree': 'code/tree',
  };

  /**
   * Parameter name corrections per command prefix.
   * LLMs guess wrong parameter names when tool descriptions are generic.
   * Maps { wrongName → correctName } for each command prefix.
   */
  private static readonly PARAM_CORRECTIONS: Record<string, Record<string, string>> = {
    'code/write': {
      'path': 'filePath',
      'file': 'filePath',
      'file_path': 'filePath',
      'filepath': 'filePath',
      'filename': 'filePath',
      'file_name': 'filePath',
      'name': 'filePath',
      'contents': 'content',
      'text': 'content',
      'body': 'content',
      'data': 'content',
      'code': 'content',
      'html': 'content',
      'source': 'content',
    },
    'code/read': {
      'path': 'filePath',
      'file': 'filePath',
      'file_path': 'filePath',
      'filepath': 'filePath',
      'filename': 'filePath',
      'name': 'filePath',
      'start': 'startLine',
      'end': 'endLine',
      'from': 'startLine',
      'to': 'endLine',
    },
    'code/edit': {
      'path': 'filePath',
      'file': 'filePath',
      'file_path': 'filePath',
      'filepath': 'filePath',
      'filename': 'filePath',
      'name': 'filePath',
      'mode': 'editMode',
      'type': 'editMode',
    },
    'code/search': {
      'query': 'pattern',
      'search': 'pattern',
      'term': 'pattern',
      'regex': 'pattern',
      'glob': 'fileGlob',
      'filter': 'fileGlob',
    },
    'code/tree': {
      'directory': 'path',
      'dir': 'path',
      'folder': 'path',
      'depth': 'maxDepth',
    },
    'code/git': {
      'subcommand': 'operation',
      'command': 'operation',
      'action': 'operation',
      'op': 'operation',
      'msg': 'message',
      'files': 'paths',
    },
  };

  /**
   * LOOP DETECTION: Track recent tool calls per persona to detect infinite loops
   * Map<personaId, Array<{hash: string, timestamp: number}>>
   * When same tool call appears 3+ times in 60 seconds, it's blocked
   */
  private static readonly recentToolCalls: Map<string, Array<{ hash: string; timestamp: number }>> = new Map();
  private static readonly LOOP_DETECTION_WINDOW_MS = 60000; // 60 seconds
  private static readonly LOOP_DETECTION_THRESHOLD = 2; // Block after 2 identical calls

  private persona: PersonaUserForToolExecutor;
  private toolRegistry: ToolRegistry;
  private formatAdapters: ToolFormatAdapter[];
  private log: ReturnType<typeof Logger.create>;
  private workspaceBootstrapped = false;

  constructor(personaUser: PersonaUserForToolExecutor) {
    this.persona = personaUser;
    this.toolRegistry = ToolRegistry.getInstance();
    this.formatAdapters = getToolFormatAdapters();

    // Per-persona tools.log in their home directory
    const category = 'logs/tools';
    this.log = Logger.create(
      `PersonaToolExecutor:${this.persona.displayName}`,
      category,
      this.persona.homeDirectory
    );
  }

  /**
   * LOOP DETECTION: Create a hash of a tool call for comparison
   */
  private static hashToolCall(toolCall: ToolCall): string {
    return `${toolCall.toolName}:${JSON.stringify(toolCall.parameters)}`;
  }

  /**
   * LOOP DETECTION: Check if a tool call is a duplicate (appears too frequently)
   * Returns true if blocked (is a loop), false if allowed
   */
  private isLoopDetected(toolCall: ToolCall): boolean {
    const personaId = this.persona.id;
    const hash = PersonaToolExecutor.hashToolCall(toolCall);
    const now = Date.now();

    // Get or create recent calls list for this persona
    let recentCalls = PersonaToolExecutor.recentToolCalls.get(personaId) || [];

    // Clean up old entries outside the window
    recentCalls = recentCalls.filter(
      entry => now - entry.timestamp < PersonaToolExecutor.LOOP_DETECTION_WINDOW_MS
    );

    // Count how many times this exact call appears
    const duplicateCount = recentCalls.filter(entry => entry.hash === hash).length;

    // Record this call (even if it will be blocked)
    recentCalls.push({ hash, timestamp: now });
    PersonaToolExecutor.recentToolCalls.set(personaId, recentCalls);

    // Block if threshold exceeded
    if (duplicateCount >= PersonaToolExecutor.LOOP_DETECTION_THRESHOLD) {
      this.log.warn(`🔁 LOOP DETECTED: ${toolCall.toolName} called ${duplicateCount + 1}x in ${PersonaToolExecutor.LOOP_DETECTION_WINDOW_MS / 1000}s - BLOCKING`);
      return true;
    }

    return false;
  }

  /**
   * Parse tool calls from AI response text using registered format adapters
   * Extensible via adapter pattern - add new formats in ToolFormatAdapter.ts
   */
  parseToolCalls(responseText: string): ToolCall[] {
    const toolCalls: ToolCall[] = [];

    // Iterate through all registered adapters
    for (const adapter of this.formatAdapters) {
      const matches = adapter.matches(responseText);

      for (const match of matches) {
        const toolCall = adapter.parse(match);
        if (toolCall) {
          toolCalls.push(toolCall);
        }
      }
    }

    return toolCalls;
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
    // Filter out looping tool calls before execution
    const filtered = toolCalls.filter(toolCall => {
      if (this.isLoopDetected(toolCall)) {
        this.log.warn(`Skipping looping tool call: ${toolCall.toolName}`);
        return false;
      }
      return true;
    });

    // Auto-bootstrap workspace if any code/* tools are being called
    if (!this.workspaceBootstrapped && this.persona.ensureCodeWorkspace) {
      const hasCodeTools = filtered.some(tc => tc.toolName.startsWith('code/'));
      if (hasCodeTools) {
        try {
          this.log.info('🔧 Auto-bootstrapping workspace for code/* tool execution');
          await this.persona.ensureCodeWorkspace();
          this.workspaceBootstrapped = true;
        } catch (err: any) {
          this.log.error(`Failed to bootstrap workspace: ${err.message}`);
        }
      }
    }

    return filtered;
  }

  /**
   * Execute a single tool call through the full pipeline.
   *
   * Handles: name/param correction, room resolution, ToolRegistry execution,
   * logging, result storage, and media collection.
   */
  private async executeSingleTool(
    toolCall: ToolCall,
    context: ToolExecutionContext,
  ): Promise<SingleToolExecution> {
    const startTime = Date.now();

    // Redirect common tool name confusion (workspace/* → code/*)
    const correctedToolName = PersonaToolExecutor.TOOL_CORRECTIONS[toolCall.toolName] ?? toolCall.toolName;
    if (correctedToolName !== toolCall.toolName) {
      this.log.info(`↪ Redirected ${toolCall.toolName} → ${correctedToolName}`);
      toolCall = { ...toolCall, toolName: correctedToolName };
    }

    // Correct common parameter name mismatches (LLMs guess wrong names)
    const paramCorrections = PersonaToolExecutor.PARAM_CORRECTIONS[toolCall.toolName];
    if (paramCorrections) {
      const correctedParams = { ...toolCall.parameters };
      for (const [wrongName, correctName] of Object.entries(paramCorrections)) {
        if (correctedParams[wrongName] !== undefined && correctedParams[correctName] === undefined) {
          correctedParams[correctName] = correctedParams[wrongName];
          delete correctedParams[wrongName];
          this.log.info(`↪ Param corrected: ${wrongName} → ${correctName}`);
        }
      }
      toolCall = { ...toolCall, parameters: correctedParams };
    }

    // Clean up code/write content: CDATA wrappers, HTML entities
    // Models encode HTML differently when writing code — normalize before execution
    if (toolCall.toolName === 'code/write' && toolCall.parameters.content) {
      let content = toolCall.parameters.content;
      let cleaned = false;

      // Strip CDATA wrappers (Together wraps HTML in <![CDATA[...]]> for XML safety)
      const cdataMatch = content.match(/^<!\[CDATA\[([\s\S]*?)\]\]>$/);
      if (cdataMatch) {
        content = cdataMatch[1];
        cleaned = true;
      }

      // Decode HTML entities in a single pass (Groq double-escapes HTML as &lt;html&gt;)
      const NAMED: Record<string, string> = { lt: '<', gt: '>', amp: '&', quot: '"', apos: "'", nbsp: ' ' };
      const decoded = content.replace(/&(#\d+|#x[\da-fA-F]+|[a-zA-Z]+);/g, (match, entity: string) => {
        if (NAMED[entity]) return NAMED[entity];
        if (entity.startsWith('#x')) return String.fromCharCode(parseInt(entity.slice(2), 16));
        if (entity.startsWith('#')) return String.fromCharCode(parseInt(entity.slice(1), 10));
        return match;
      });
      if (decoded !== content) { content = decoded; cleaned = true; }

      if (cleaned) {
        toolCall = { ...toolCall, parameters: { ...toolCall.parameters, content } };
        this.log.info('↪ Cleaned code/write content (CDATA/entity normalization)');
      }
    }

    // Resolve "current" room parameter to actual room name
    const resolvedParams = await this.resolveRoomParameters(toolCall.parameters, context.contextId);

    // Inject userId (standard CommandParams field) and contextId
    const paramsWithCaller = {
      ...resolvedParams,
      userId: context.personaId,    // Standard CommandParams.userId — THE identity field
      personaId: context.personaId, // Backward compat (ai/sleep, ai/should-respond-fast)
      contextId: context.contextId  // Room/context scope
    };

    // Log tool call with clean params formatting (not array-wrapped)
    const paramsJson = JSON.stringify(paramsWithCaller, null, 2);
    this.log.info(`┌─ CALL: ${toolCall.toolName}`);
    this.log.info(`│  params: ${paramsJson.replace(/\n/g, '\n│  ')}`);

    // Use ToolRegistry for ALL commands - no special cases
    // NO try-catch - let exceptions bubble to PersonaResponseGenerator
    // ToolRegistry returns {success: false, error} for expected failures
    const registryResult = await this.toolRegistry.executeTool(
      toolCall.toolName,
      paramsWithCaller,  // Pass params with callerId injected
      context.sessionId,  // Pass AI's sessionId for proper attribution
      context.contextId,
      context.context  // Pass PersonaUser's enriched context (with callerType='persona')
    );

    const result: ToolResult = {
      toolName: registryResult.toolName,
      success: registryResult.success,
      content: registryResult.content,
      media: registryResult.media,  // ← Preserve structured media
      error: registryResult.error
    };

    const duration = Date.now() - startTime;

    // Log result with clear visual structure
    if (result.success) {
      // Parse result for better display (show key fields if JSON)
      let resultSummary = result.content?.slice(0, 500) || 'no content';
      try {
        const parsed = JSON.parse(result.content || '');
        // Extract key fields for readable summary
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

      this.log.info(`└─ RESULT: ✓ ${duration}ms`);
      this.log.info(`   ${resultSummary}${result.content && result.content.length > 500 ? '...' : ''}`);
      if (result.media && result.media.length > 0) {
        this.log.info(`   media: ${result.media.map(m => `${m.type} (${m.mimeType})`).join(', ')}`);
      }
    } else {
      this.log.error(`└─ RESULT: ✗ ${duration}ms`);
      this.log.error(`   error: ${result.error || 'unknown error'}`);
    }

    // Store tool result in working memory and get UUID
    this.log.debugIf(() => [`${toolCall.toolName} returned media:`, result.media ? `${result.media.length} items` : 'NONE']);
    if (result.media && result.media.length > 0) {
      this.log.debugIf(() => ['Media details:', result.media!.map(m => ({
        type: m.type,
        hasBase64: !!m.base64,
        base64Length: m.base64?.length,
        mimeType: m.mimeType,
        hasUrl: !!m.url
      }))]);
    }

    // Store tool result (awaited to get UUID, but could be fire-and-forget if needed)
    const resultId = await this.storeToolResult(
      toolCall.toolName,
      toolCall.parameters,
      {
        success: result.success,
        data: result.content,  // Store full content in metadata
        error: result.error,
        media: result.media  // Pass media for storage and RAG context
      },
      context.contextId  // Use contextId (room) for storage
    );
    this.log.debug(`Stored tool result #${resultId.slice(0, 8)} with ${result.media?.length || 0} media`);

    // Collect media for this tool
    const collectedMedia: MediaItem[] = [];

    // Check if THIS persona wants media
    // IMPORTANT: If AI explicitly called screenshot tool, they want the image!
    // So we pass through media for screenshot regardless of autoLoadMedia config
    const isScreenshotTool = toolCall.toolName === 'screenshot' || toolCall.toolName === 'interface/screenshot';
    const shouldLoadMedia = context.personaConfig.autoLoadMedia || isScreenshotTool;

    if (result.media && shouldLoadMedia) {
      // Filter by supported types (unless it's screenshot - then pass through images)
      const supportedMedia = result.media.filter(m =>
        isScreenshotTool || context.personaConfig.supportedMediaTypes.includes(m.type)
      );

      if (supportedMedia.length > 0) {
        this.log.info(`Loading ${supportedMedia.length} media (types: ${supportedMedia.map(m => m.type).join(', ')})${isScreenshotTool ? ' [screenshot override]' : ''}`);
        collectedMedia.push(...supportedMedia);
      }
    } else if (result.media && result.media.length > 0) {
      this.log.debug(`Skipping ${result.media.length} media (autoLoadMedia=false)`);
    }

    // Fire-and-forget: Log tool execution to cognition database (non-blocking)
    // This is telemetry - don't block the response pipeline for it
    CognitionLogger.logToolExecution(
      this.persona.id,
      this.persona.displayName,
      toolCall.toolName,
      toolCall.parameters,
      result.success ? 'success' : 'error',
      duration,
      'chat',  // Domain
      context.contextId,
      {
        toolResult: result.content?.slice(0, 1000),  // First 1000 chars of result
        errorMessage: result.error,
        storedResultId: resultId  // Phase 3B: Link to stored result
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
   *
   * @param toolCalls - Array of parsed tool calls
   * @param context - Execution context with media configuration
   * @returns Object with formatted text results, optional media array, and stored result UUIDs
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

    // Execute all tools concurrently — O(max tool time) instead of O(sum)
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
   *
   * Calls executeSingleTool directly — no XML serialization/deserialization round-trip.
   * Full content is returned (not summaries). Truncated honestly if too large.
   *
   * @param nativeToolCalls - Tool calls from AI provider (with id, name, input)
   * @param context - Execution context with persona/session info
   * @param maxResultChars - Maximum characters per tool result (truncated honestly)
   * @returns Per-tool results, media, and stored IDs
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

    // Convert native format → executor format (decode sanitized names, stringify params)
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

    // Map results back to native tool calls with tool_use_id correlation.
    // Tools blocked by loop detection get error results.
    const filteredSet = new Set(filtered);
    const results: NativeToolResult[] = [];
    let execIdx = 0;

    for (let i = 0; i < nativeToolCalls.length; i++) {
      if (!filteredSet.has(executorCalls[i])) {
        // Tool was blocked by loop detection
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

      // Truncate honestly (not summarize) if too large
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
      // Wrap error in code block for better UI readability
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
   * Resolve "current" room parameters to actual room names
   * Handles any parameter named "room" that has value "current"
   *
   * @param params - Tool parameters from AI
   * @param contextId - The contextId (roomId) from execution context
   * @returns Parameters with resolved room values
   */
  private async resolveRoomParameters(
    params: Record<string, string>,
    contextId: UUID
  ): Promise<Record<string, string>> {
    const resolved = { ...params };

    // Check if there's a room parameter that needs resolution
    if (resolved.room === 'current') {
      const roomName = await RoomResolver.resolveCurrentParam('current', contextId);
      if (roomName) {
        this.log.info(`Resolved room="current" to "${roomName}"`);
        resolved.room = roomName;
      } else {
        this.log.warn(`Could not resolve room="current" from contextId ${contextId}`);
      }
    }

    return resolved;
  }

  /**
   * Strip tool blocks from response text to get clean user-facing message
   * Uses adapters to find and remove all tool blocks
   */
  stripToolBlocks(responseText: string): string {
    let cleaned = responseText;

    // Find all tool blocks using adapters
    const allMatches: Array<{ start: number; end: number }> = [];

    for (const adapter of this.formatAdapters) {
      const matches = adapter.matches(cleaned);
      for (const match of matches) {
        allMatches.push({ start: match.startIndex, end: match.endIndex });
      }
    }

    // Sort by start index descending (remove from end first to preserve indices)
    allMatches.sort((a, b) => b.start - a.start);

    // Remove all matched blocks
    for (const match of allMatches) {
      cleaned = cleaned.slice(0, match.start) + cleaned.slice(match.end);
    }

    return cleaned.trim();
  }

  /**
   * Get list of available tools (from ToolRegistry)
   */
  getAvailableTools(): string[] {
    return this.toolRegistry.getAllTools().map(t => t.name);
  }

  /**
   * Store tool result as ChatMessageEntity for working memory
   * Phase 3B: Lazy loading pattern - store full data, return UUID for reference
   *
   * @param toolName - Name of tool executed
   * @param parameters - Parameters passed to tool
   * @param result - Execution result with optional media
   * @param roomId - Room where tool was executed
   * @returns UUID of stored message entity
   */
  async storeToolResult(
    toolName: string,
    parameters: Record<string, unknown>,
    result: { success: boolean; data: unknown; error?: string; media?: MediaItem[] },
    roomId: UUID
  ): Promise<UUID> {
    // Generate short summary (< 100 tokens)
    const summary = this.generateSummary(toolName, result);

    // Create message entity
    const message = new ChatMessageEntity();
    message.id = generateUUID();
    message.roomId = roomId;
    message.senderId = this.persona.id;
    message.senderName = this.persona.displayName;
    message.senderType = 'system';  // Tool results are system messages
    message.content = { text: summary, media: result.media || [] };  // Include media from tool results
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

    // Store via Commands system (universal pattern)
    await DataCreate.execute<ChatMessageEntity>({
        collection: ChatMessageEntity.collection,
        backend: 'server',
        data: message
      }
    );

    this.log.debug(`Stored tool result #${message.id.slice(0, 8)} (${summary})`);
    return message.id;
  }

  /**
   * Generate short summary of tool result for RAG context
   * Phase 3B: Keep summaries < 100 tokens to save context budget
   *
   * @param toolName - Name of tool executed
   * @param result - Execution result
   * @returns Short summary string
   */
  private generateSummary(
    toolName: string,
    result: { success: boolean; data: unknown; error?: unknown }
  ): string {
    if (!result.success) {
      const errorMessage = this.stringifyError(result.error);
      return `Tool '${toolName}' failed: ${errorMessage}`;
    }

    const data = result.data;

    // Action label from tool name: "code/write" → "write", "collaboration/decision/vote" → "vote"
    const action = toolName.split('/').pop() ?? toolName;

    // Data-shape-driven summary — extract what the data reveals, not what tool produced it
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

      // File path (most common structured field)
      const filePath = obj.filePath ?? obj.file_path ?? obj.path ?? obj.fileName ?? obj.file_name;
      if (filePath) parts.push(String(filePath));

      // Size / count metrics
      const bytes = obj.bytesWritten ?? obj.bytes_written ?? obj.size ?? obj.byteLength;
      if (typeof bytes === 'number') parts.push(`${bytes} bytes`);

      const count = obj.count ?? obj.total ?? obj.matches ?? obj.length;
      if (typeof count === 'number') parts.push(`${count} items`);

      // Dimensions
      const width = obj.width;
      const height = obj.height;
      if (typeof width === 'number' && typeof height === 'number') parts.push(`${width}x${height}`);

      if (parts.length > 0) return `${action}: ${parts.join(', ')}`;
    }

    // Compact fallback — tool name + truncated preview
    const dataStr = typeof data === 'string' ? data : JSON.stringify(data);
    return `${action}: ${dataStr.slice(0, 120)}${dataStr.length > 120 ? '...' : ''}`;
  }

  /**
   * Convert any error value to a human-readable string
   * Prevents [object Object] in error messages
   *
   * @param error - Any error value (string, Error, object, etc.)
   * @returns Human-readable error string
   */
  private stringifyError(error: unknown): string {
    if (error === undefined || error === null) {
      return 'Unknown error';
    }

    // Already a string
    if (typeof error === 'string') {
      return error;
    }

    // Error instance
    if (error instanceof Error) {
      return error.message;
    }

    // Object with message property (common pattern)
    if (typeof error === 'object' && error !== null) {
      const obj = error as Record<string, unknown>;

      // Try common error message properties
      if (typeof obj.message === 'string') return obj.message;
      if (typeof obj.error === 'string') return obj.error;
      if (typeof obj.errorMessage === 'string') return obj.errorMessage;
      if (typeof obj.msg === 'string') return obj.msg;

      // Nested error object
      if (obj.error && typeof obj.error === 'object') {
        const nested = obj.error as Record<string, unknown>;
        if (typeof nested.message === 'string') return nested.message;
      }

      // Last resort: stringify the object
      try {
        const str = JSON.stringify(error);
        // Don't return huge objects
        if (str.length > 500) {
          return `${str.slice(0, 500)}...`;
        }
        return str;
      } catch {
        return 'Error object could not be serialized';
      }
    }

    // Fallback for primitives
    return String(error);
  }
}
