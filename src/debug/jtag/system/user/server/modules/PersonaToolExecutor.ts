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
import { Logger } from '../../../core/logging/Logger';
import { RoomResolver } from '../../../core/server/RoomResolver';

import { DataCreate } from '../../../../commands/data/create/shared/DataCreateTypes';
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
 * PersonaToolExecutor - Clean tool execution via ToolRegistry
 */
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
   * LOOP DETECTION: Track recent tool calls per persona to detect infinite loops
   * Map<personaId, Array<{hash: string, timestamp: number}>>
   * When same tool call appears 3+ times in 60 seconds, it's blocked
   */
  private static readonly recentToolCalls: Map<string, Array<{ hash: string; timestamp: number }>> = new Map();
  private static readonly LOOP_DETECTION_WINDOW_MS = 60000; // 60 seconds
  private static readonly LOOP_DETECTION_THRESHOLD = 3; // Block after 3 identical calls

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

  /**
   * Execute tool calls and return formatted results + optional media
   * Phase 3B: Now also stores results as ChatMessageEntity and returns UUIDs
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
    storedResultIds: UUID[];  // Phase 3B: UUIDs for lazy loading
  }> {
    if (toolCalls.length === 0) {
      return { formattedResults: '', storedResultIds: [] };
    }

    this.log.info(`Executing ${toolCalls.length} tool(s): ${toolCalls.map(t => t.toolName).join(', ')}`);

    // Filter out looping tool calls before execution
    const filteredToolCalls = toolCalls.filter(toolCall => {
      if (this.isLoopDetected(toolCall)) {
        this.log.warn(`Skipping looping tool call: ${toolCall.toolName}`);
        return false;
      }
      return true;
    });

    if (filteredToolCalls.length === 0) {
      this.log.warn('All tool calls blocked by loop detection');
      return { formattedResults: '[All tool calls blocked - infinite loop detected]', storedResultIds: [] };
    }

    // Auto-bootstrap workspace if any code/* tools are being called
    if (!this.workspaceBootstrapped && this.persona.ensureCodeWorkspace) {
      const hasCodeTools = filteredToolCalls.some(tc => tc.toolName.startsWith('code/'));
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

    // PARALLELIZED: Execute all tools concurrently instead of sequentially
    // This reduces tool execution time from O(sum of all tool times) to O(max tool time)
    // Example: 3 tools × 500ms each = 1500ms sequential → 500ms parallel (3x speedup)
    const toolExecutionPromises = filteredToolCalls.map(async (toolCall) => {
      const startTime = Date.now();

      // Redirect common tool name confusion (workspace/* → code/*)
      // LLMs sometimes confuse workspace/tree (command hierarchy) with code/tree (file system)
      const correctedToolName = PersonaToolExecutor.TOOL_CORRECTIONS[toolCall.toolName] ?? toolCall.toolName;
      if (correctedToolName !== toolCall.toolName) {
        this.log.info(`↪ Redirected ${toolCall.toolName} → ${correctedToolName}`);
        toolCall = { ...toolCall, toolName: correctedToolName };
      }

      // Resolve "current" room parameter to actual room name
      // This handles wall/*, chat/*, and any other room-scoped commands
      const resolvedParams = await this.resolveRoomParameters(toolCall.parameters, context.contextId);

      // Inject userId (standard CommandParams field) and contextId
      // userId is the persona's UUID — the canonical identity field on CommandParams
      // personaId kept for backward compat with ai/sleep, ai/should-respond-fast
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

      // Phase 3B: Store tool result in working memory and get UUID
      // Fire-and-forget pattern: storage is non-critical, don't block on it
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

      return {
        result,
        resultId,
        media: collectedMedia,
        formattedResult: this.formatToolResult(result)
      };
    });

    // Wait for all tool executions to complete in parallel
    const toolResults = await Promise.all(toolExecutionPromises);

    // Aggregate results maintaining original order
    const results: string[] = [];
    const allMedia: MediaItem[] = [];
    const storedResultIds: UUID[] = [];

    for (const { result, resultId, media, formattedResult } of toolResults) {
      results.push(formattedResult);
      storedResultIds.push(resultId);
      allMedia.push(...media);
    }

    const successCount = toolResults.filter(r => r.result.success).length;
    this.log.info(`Complete: ${successCount}/${toolCalls.length} successful, ${allMedia.length} media loaded, ${storedResultIds.length} stored`);

    return {
      formattedResults: results.join('\n\n'),
      media: allMedia.length > 0 ? allMedia : undefined,
      storedResultIds  // Phase 3B: Return UUIDs for lazy loading
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
      // Don't truncate error messages - AIs need full context to debug
      // IMPORTANT: Properly stringify error objects to avoid [object Object]
      const errorMessage = this.stringifyError(result.error);
      return `Tool '${toolName}' failed: ${errorMessage}`;
    }

    // Tool-specific summarization logic
    const data = result.data;

    if (toolName === 'grep' || toolName === 'code/pattern-search') {
      const text = typeof data === 'string' ? data : JSON.stringify(data);
      const lines = text.split('\n').filter(l => l.trim()).length;
      return `grep found ${lines} match${lines !== 1 ? 'es' : ''}`;
    }

    if (toolName === 'screenshot') {
      const img = data as any;
      if (img?.width && img?.height) {
        return `Screenshot captured (${img.width}x${img.height}px)`;
      }
      return 'Screenshot captured';
    }

    if (toolName === DATA_COMMANDS.LIST) {
      const items = data as any[];
      const count = Array.isArray(items) ? items.length : 0;
      return `${DATA_COMMANDS.LIST} returned ${count} item${count !== 1 ? 's' : ''}`;
    }

    if (toolName === DATA_COMMANDS.READ) {
      // When fetching tool results from working memory, don't output raw JSON
      // Just acknowledge the retrieval
      return 'Retrieved data from working memory';
    }

    if (toolName === 'code/read' || toolName === 'file/load') {
      const text = typeof data === 'string' ? data : JSON.stringify(data);
      const lines = text.split('\n').length;
      return `Read ${lines} lines from file`;
    }

    if (toolName === 'bash' || toolName === 'shell/execute') {
      const output = typeof data === 'string' ? data : JSON.stringify(data);
      const lines = output.split('\n').length;
      return `Command executed (${lines} lines of output)`;
    }

    // Generic summary for unknown tools - give AIs enough context to work with
    const dataStr = typeof data === 'string' ? data : JSON.stringify(data, null, 2);
    const preview = dataStr.slice(0, 500);
    return `Tool '${toolName}' completed: ${preview}${dataStr.length > 500 ? '...' : ''}`;
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
