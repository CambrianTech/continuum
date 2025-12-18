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
import { Commands } from '../../../core/shared/Commands';
import type { DataCreateParams, DataCreateResult } from '../../../../commands/data/create/shared/DataCreateTypes';
import { getToolFormatAdapters, type ToolFormatAdapter } from './ToolFormatAdapter';
import { Logger, FileMode } from '../../../core/logging/Logger';
import { SystemPaths } from '../../../core/config/SystemPaths';
import { RoomResolver } from '../../../core/server/RoomResolver';
import * as fs from 'fs';
import * as path from 'path';

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
  readonly entity: {
    readonly uniqueId: string;
  };
}

export class PersonaToolExecutor {
  private static readonly COGNITION_LOG_PATH = path.join(process.cwd(), '.continuum/jtag/system/logs/cognition.log');

  private persona: PersonaUserForToolExecutor;
  private toolRegistry: ToolRegistry;
  private formatAdapters: ToolFormatAdapter[];
  private log: ReturnType<typeof Logger.create>;

  constructor(personaUser: PersonaUserForToolExecutor) {
    this.persona = personaUser;
    this.toolRegistry = ToolRegistry.getInstance();
    this.formatAdapters = getToolFormatAdapters();

    // Per-persona tools.log in their own directory (works like daemon logs)
    const category = `personas/${this.persona.entity.uniqueId}/logs/tools`;
    this.log = Logger.create(`PersonaToolExecutor:${this.persona.displayName}`, category);
  }

  /**
   * Log to dedicated cognition file (separate from main logs)
   * @deprecated Use Logger instead for categorized logging
   */
  private static logToCognitionFile(message: string): void {
    const timestamp = new Date().toISOString();
    const logLine = `[${timestamp}] ${message}\n`;
    fs.appendFileSync(PersonaToolExecutor.COGNITION_LOG_PATH, logLine, 'utf8');
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
    PersonaToolExecutor.logToCognitionFile(`🔧 ${this.persona.displayName}: [TOOL] Executing ${toolCalls.length} tool(s): ${toolCalls.map(t => t.toolName).join(', ')}`);

    // PARALLELIZED: Execute all tools concurrently instead of sequentially
    // This reduces tool execution time from O(sum of all tool times) to O(max tool time)
    // Example: 3 tools × 500ms each = 1500ms sequential → 500ms parallel (3x speedup)
    const toolExecutionPromises = toolCalls.map(async (toolCall) => {
      const startTime = Date.now();

      // Resolve "current" room parameter to actual room name
      // This handles wall/*, chat/*, and any other room-scoped commands
      const resolvedParams = await this.resolveRoomParameters(toolCall.parameters, context.contextId);

      this.log.info(`Calling ${toolCall.toolName} with params:`, resolvedParams);
      PersonaToolExecutor.logToCognitionFile(`🔧 ${this.persona.displayName}: [TOOL] ${toolCall.toolName} | params: ${JSON.stringify(resolvedParams)}`);

      // Use ToolRegistry for ALL commands - no special cases
      // NO try-catch - let exceptions bubble to PersonaResponseGenerator
      // ToolRegistry returns {success: false, error} for expected failures
      const registryResult = await this.toolRegistry.executeTool(
        toolCall.toolName,
        resolvedParams,
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

      if (result.success) {
        this.log.info(`${toolCall.toolName} ✓ ${duration}ms → ${result.content?.slice(0, 200) || 'no content'}${result.content && result.content.length > 200 ? '...' : ''}`);
        if (result.media && result.media.length > 0) {
          this.log.info(`  └─ Media: ${result.media.map(m => `${m.type} (${m.mimeType})`).join(', ')}`);
        }
      } else {
        this.log.error(`${toolCall.toolName} ✗ ${duration}ms`);
        this.log.error(`  └─ Error: ${result.error || 'unknown error'}`);
        this.log.error(`  └─ Params:`, toolCall.parameters);
      }
      PersonaToolExecutor.logToCognitionFile(`${result.success ? '✅' : '❌'} ${this.persona.displayName}: [TOOL] ${toolCall.toolName} ${result.success ? 'success' : 'failed'} (${duration}ms, ${result.content?.length || 0} chars, media: ${result.media?.length || 0})`);

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
      const isScreenshotTool = toolCall.toolName === 'screenshot';
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
    await Commands.execute<DataCreateParams<ChatMessageEntity>, DataCreateResult<ChatMessageEntity>>(
      DATA_COMMANDS.CREATE,
      {
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
    result: { success: boolean; data: unknown; error?: string }
  ): string {
    if (!result.success) {
      return `Tool '${toolName}' failed: ${result.error?.slice(0, 100) || 'Unknown error'}`;
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

    // Generic summary for unknown tools
    const dataStr = typeof data === 'string' ? data : JSON.stringify(data);
    const preview = dataStr.slice(0, 80);
    return `Tool '${toolName}' completed: ${preview}${dataStr.length > 80 ? '...' : ''}`;
  }
}
