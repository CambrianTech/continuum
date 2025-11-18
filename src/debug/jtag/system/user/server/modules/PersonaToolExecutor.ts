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

/**
 * Parsed tool call from AI response
 */
export interface ToolCall {
  toolName: string;
  parameters: Record<string, string>;
}

/**
 * Tool execution result
 */
export interface ToolResult {
  toolName: string;
  success: boolean;
  content?: string;
  error?: string;
}

/**
 * PersonaToolExecutor - Clean tool execution via ToolRegistry
 */
export class PersonaToolExecutor {
  private personaId: UUID;
  private personaName: string;
  private toolRegistry: ToolRegistry;

  constructor(personaId: UUID, personaName: string) {
    this.personaId = personaId;
    this.personaName = personaName;
    this.toolRegistry = ToolRegistry.getInstance();
  }

  /**
   * Parse tool calls from AI response text
   * Extracts <tool_use> XML blocks
   */
  parseToolCalls(responseText: string): ToolCall[] {
    const toolCalls: ToolCall[] = [];

    // Match <tool_use>...</tool_use> blocks
    const toolUseRegex = /<tool_use>(.*?)<\/tool_use>/gs;
    const matches = responseText.matchAll(toolUseRegex);

    for (const match of matches) {
      const toolBlock = match[1];

      // Extract tool_name
      const toolNameMatch = toolBlock.match(/<tool_name>(.*?)<\/tool_name>/s);
      if (!toolNameMatch) continue;
      const toolName = toolNameMatch[1].trim();

      // Extract parameters
      const parameters: Record<string, string> = {};
      const parametersMatch = toolBlock.match(/<parameters>(.*?)<\/parameters>/s);
      if (parametersMatch) {
        const paramsBlock = parametersMatch[1];

        // Extract individual parameter tags
        const paramRegex = /<(\w+)>(.*?)<\/\1>/gs;
        const paramMatches = paramsBlock.matchAll(paramRegex);

        for (const paramMatch of paramMatches) {
          const paramName = paramMatch[1];
          const paramValue = paramMatch[2].trim();
          parameters[paramName] = paramValue;
        }
      }

      toolCalls.push({ toolName, parameters });
    }

    return toolCalls;
  }

  /**
   * Execute tool calls and return formatted results
   */
  async executeToolCalls(toolCalls: ToolCall[], contextId: UUID): Promise<string> {
    if (toolCalls.length === 0) return '';

    console.log(`🔧 ${this.personaName}: [TOOL] Executing ${toolCalls.length} tool(s): ${toolCalls.map(t => t.toolName).join(', ')}`);

    const results: string[] = [];

    for (const toolCall of toolCalls) {
      const startTime = Date.now();

      console.log(`🔧 ${this.personaName}: [TOOL] ${toolCall.toolName}`, toolCall.parameters);

      // Use ToolRegistry for ALL commands - no special cases
      // NO try-catch - let exceptions bubble to PersonaResponseGenerator
      // ToolRegistry returns {success: false, error} for expected failures
      const registryResult = await this.toolRegistry.executeTool(
        toolCall.toolName,
        toolCall.parameters,
        contextId
      );

      const result: ToolResult = {
        toolName: registryResult.toolName,
        success: registryResult.success,
        content: registryResult.content,
        error: registryResult.error
      };

      const duration = Date.now() - startTime;

      console.log(`${result.success ? '✅' : '❌'} ${this.personaName}: [TOOL] ${toolCall.toolName} ${result.success ? 'success' : 'failed'} (${duration}ms, ${result.content?.length || 0} chars)`);

      // Log tool execution to cognition database (for interrogation)
      await CognitionLogger.logToolExecution(
        this.personaId,
        this.personaName,
        toolCall.toolName,
        toolCall.parameters,
        result.success ? 'success' : 'error',
        duration,
        'chat',  // Domain
        contextId,
        {
          toolResult: result.content?.slice(0, 1000),  // First 1000 chars of result
          errorMessage: result.error
        }
      );

      results.push(this.formatToolResult(result));
    }

    const successCount = results.filter(r => r.includes('<status>success</status>')).length;
    console.log(`🏁 ${this.personaName}: [TOOL] Complete: ${successCount}/${toolCalls.length} successful`);

    return results.join('\n\n');
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
    return responseText.replace(/<tool_use>.*?<\/tool_use>/gs, '').trim();
  }

  /**
   * Get list of available tools (from ToolRegistry)
   */
  getAvailableTools(): string[] {
    return this.toolRegistry.getAllTools().map(t => t.name);
  }
}
