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
import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { ToolRegistry } from '../../../tools/server/ToolRegistry';
import type { MediaItem } from '../../../data/entities/ChatMessageEntity';
import type { PersonaMediaConfig } from './PersonaMediaConfig';
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
export class PersonaToolExecutor {
  private static readonly COGNITION_LOG_PATH = path.join(process.cwd(), '.continuum/jtag/system/logs/cognition.log');

  private personaId: UUID;
  private personaName: string;
  private toolRegistry: ToolRegistry;

  constructor(personaId: UUID, personaName: string) {
    this.personaId = personaId;
    this.personaName = personaName;
    this.toolRegistry = ToolRegistry.getInstance();
  }

  /**
   * Log to dedicated cognition file (separate from main logs)
   */
  private static logToCognitionFile(message: string): void {
    const timestamp = new Date().toISOString();
    const logLine = `[${timestamp}] ${message}\n`;
    fs.appendFileSync(PersonaToolExecutor.COGNITION_LOG_PATH, logLine, 'utf8');
  }

  /**
   * Parse tool calls from AI response text
   * Extracts <tool name="..."> XML blocks
   */
  parseToolCalls(responseText: string): ToolCall[] {
    const toolCalls: ToolCall[] = [];

    // Match <tool name="...">...</tool> blocks
    const toolRegex = /<tool\s+name="([^"]+)">(.*?)<\/tool>/gs;
    const matches = responseText.matchAll(toolRegex);

    for (const match of matches) {
      const toolName = match[1].trim();
      const toolBlock = match[2];

      // Extract parameters - direct children tags
      const parameters: Record<string, string> = {};
      const paramRegex = /<(\w+)>(.*?)<\/\1>/gs;
      const paramMatches = toolBlock.matchAll(paramRegex);

      for (const paramMatch of paramMatches) {
        const paramName = paramMatch[1];
        const paramValue = paramMatch[2].trim();
        parameters[paramName] = paramValue;
      }

      toolCalls.push({ toolName, parameters });
    }

    return toolCalls;
  }

  /**
   * Execute tool calls and return formatted results + optional media
   *
   * @param toolCalls - Array of parsed tool calls
   * @param context - Execution context with media configuration
   * @returns Object with formatted text results and optional media array
   */
  async executeToolCalls(
    toolCalls: ToolCall[],
    context: ToolExecutionContext
  ): Promise<{
    formattedResults: string;
    media?: MediaItem[];
  }> {
    if (toolCalls.length === 0) {
      return { formattedResults: '' };
    }

    console.log(`🔧 ${this.personaName}: [TOOL] Executing ${toolCalls.length} tool(s): ${toolCalls.map(t => t.toolName).join(', ')}`);
    PersonaToolExecutor.logToCognitionFile(`🔧 ${this.personaName}: [TOOL] Executing ${toolCalls.length} tool(s): ${toolCalls.map(t => t.toolName).join(', ')}`);

    const results: string[] = [];
    const allMedia: MediaItem[] = [];

    for (const toolCall of toolCalls) {
      const startTime = Date.now();

      console.log(`🔧 ${this.personaName}: [TOOL] ${toolCall.toolName}`, toolCall.parameters);
      PersonaToolExecutor.logToCognitionFile(`🔧 ${this.personaName}: [TOOL] ${toolCall.toolName} | params: ${JSON.stringify(toolCall.parameters)}`);

      // Use ToolRegistry for ALL commands - no special cases
      // NO try-catch - let exceptions bubble to PersonaResponseGenerator
      // ToolRegistry returns {success: false, error} for expected failures
      const registryResult = await this.toolRegistry.executeTool(
        toolCall.toolName,
        toolCall.parameters,
        context.sessionId,  // Pass AI's sessionId for proper attribution
        context.contextId
      );

      const result: ToolResult = {
        toolName: registryResult.toolName,
        success: registryResult.success,
        content: registryResult.content,
        media: registryResult.media,  // ← Preserve structured media
        error: registryResult.error
      };

      const duration = Date.now() - startTime;

      console.log(`${result.success ? '✅' : '❌'} ${this.personaName}: [TOOL] ${toolCall.toolName} ${result.success ? 'success' : 'failed'} (${duration}ms, ${result.content?.length || 0} chars)`);
      PersonaToolExecutor.logToCognitionFile(`${result.success ? '✅' : '❌'} ${this.personaName}: [TOOL] ${toolCall.toolName} ${result.success ? 'success' : 'failed'} (${duration}ms, ${result.content?.length || 0} chars, media: ${result.media?.length || 0})`);

      // Check if THIS persona wants media
      if (result.media && context.personaConfig.autoLoadMedia) {
        // Filter by supported types
        const supportedMedia = result.media.filter(m =>
          context.personaConfig.supportedMediaTypes.includes(m.type)
        );

        if (supportedMedia.length > 0) {
          console.log(`📸 ${this.personaName}: [MEDIA] Loading ${supportedMedia.length} media item(s) (types: ${supportedMedia.map(m => m.type).join(', ')})`);
          allMedia.push(...supportedMedia);
        }
      } else if (result.media && result.media.length > 0) {
        console.log(`🚫 ${this.personaName}: [MEDIA] Skipping ${result.media.length} media item(s) (autoLoadMedia=false)`);
      }

      // Log tool execution to cognition database (for interrogation)
      await CognitionLogger.logToolExecution(
        this.personaId,
        this.personaName,
        toolCall.toolName,
        toolCall.parameters,
        result.success ? 'success' : 'error',
        duration,
        'chat',  // Domain
        context.contextId,
        {
          toolResult: result.content?.slice(0, 1000),  // First 1000 chars of result
          errorMessage: result.error
        }
      );

      // Always include text description (for non-vision AIs or logging)
      results.push(this.formatToolResult(result));
    }

    const successCount = results.filter(r => r.includes('<status>success</status>')).length;
    console.log(`🏁 ${this.personaName}: [TOOL] Complete: ${successCount}/${toolCalls.length} successful, ${allMedia.length} media item(s) loaded`);

    return {
      formattedResults: results.join('\n\n'),
      media: allMedia.length > 0 ? allMedia : undefined
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
   * Strip tool blocks from response text to get clean user-facing message
   */
  stripToolBlocks(responseText: string): string {
    return responseText.replace(/<tool\s+name="[^"]+">.*?<\/tool>/gs, '').trim();
  }

  /**
   * Get list of available tools (from ToolRegistry)
   */
  getAvailableTools(): string[] {
    return this.toolRegistry.getAllTools().map(t => t.name);
  }
}
