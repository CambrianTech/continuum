/**
 * ToolFormatAdapter - Abstract base class for tool call format parsing
 *
 * Enables extensible support for multiple XML/HTML tool calling formats.
 * Each adapter implements its own regex matcher and parse logic.
 *
 * Usage:
 * 1. Extend this class
 * 2. Implement matches() with your regex
 * 3. Implement parse() with your extraction logic
 * 4. Use protected helpers for common XML parsing tasks
 */

export interface ToolCall {
  toolName: string;
  parameters: Record<string, string>;
}

export interface ToolCallMatch {
  fullMatch: string;
  startIndex: number;
  endIndex: number;
}

/**
 * Tool definition from ToolRegistry
 */
export interface ToolDefinition {
  name: string;
  description: string;
  parameters: {
    type: 'object';
    properties: Record<string, { type: string; description: string; required?: boolean }>;
    required: string[];
  };
  category?: string;
}

/**
 * Abstract adapter for bidirectional tool format handling
 * Handles BOTH directions:
 * 1. RAG → AI: Format tools for system prompt
 * 2. AI → System: Parse tool calls from response
 */
export abstract class ToolFormatAdapter {
  /**
   * Unique identifier for this adapter format
   */
  abstract readonly formatName: string;

  /**
   * Format tools for system prompt (RAG → AI direction)
   * Each adapter formats tools in the way their target model expects
   *
   * @param tools - Available tool definitions
   * @returns Formatted string for system prompt
   */
  abstract formatToolsForPrompt(tools: ToolDefinition[]): string;

  /**
   * Format tool results for context injection (System → AI direction)
   * Each adapter formats results in the way their target model expects
   *
   * @param results - Executed tool results
   * @returns Formatted string for context
   */
  abstract formatResultsForContext(results: Array<{ toolName: string; success: boolean; content?: string; error?: string }>): string;

  /**
   * Match all tool calls in text using this adapter's regex (AI → System direction)
   *
   * @param text - Response text containing tool calls
   * @returns Array of matches with position info
   */
  abstract matches(text: string): ToolCallMatch[];

  /**
   * Parse a matched tool call into structured ToolCall (AI → System direction)
   *
   * @param match - Matched tool call text
   * @returns Parsed ToolCall with name and parameters
   */
  abstract parse(match: ToolCallMatch): ToolCall | null;

  /**
   * Protected helper: Extract parameter tags from XML block
   * Finds all <paramName>value</paramName> pairs
   *
   * @param xmlBlock - XML text containing parameter tags
   * @returns Record of parameter name -> value
   */
  protected extractParameters(xmlBlock: string): Record<string, string> {
    const parameters: Record<string, string> = {};
    const paramRegex = /<(\w+)>(.*?)<\/\1>/gs;
    const paramMatches = xmlBlock.matchAll(paramRegex);

    for (const paramMatch of paramMatches) {
      const paramName = paramMatch[1];
      const paramValue = paramMatch[2].trim();
      parameters[paramName] = paramValue;
    }

    return parameters;
  }

  /**
   * Protected helper: Extract text content from specific XML tag
   *
   * @param xmlBlock - XML text
   * @param tagName - Tag name to extract
   * @returns Tag content or null if not found
   */
  protected extractTag(xmlBlock: string, tagName: string): string | null {
    const regex = new RegExp(`<${tagName}>(.*?)<\/${tagName}>`, 's');
    const match = xmlBlock.match(regex);
    return match ? match[1].trim() : null;
  }

  /**
   * Protected helper: Extract attribute value from opening tag
   *
   * @param xmlBlock - XML text
   * @param attributeName - Attribute name to extract
   * @returns Attribute value or null if not found
   */
  protected extractAttribute(xmlBlock: string, attributeName: string): string | null {
    const regex = new RegExp(`${attributeName}="([^"]+)"`, 'i');
    const match = xmlBlock.match(regex);
    return match ? match[1].trim() : null;
  }
}

/**
 * Adapter for old-style format: <tool name="...">...</tool>
 */
export class OldStyleToolAdapter extends ToolFormatAdapter {
  readonly formatName = 'old-style';

  formatToolsForPrompt(tools: ToolDefinition[]): string {
    let output = 'Available Tools:\n\n';

    for (const tool of tools) {
      output += `Tool: ${tool.name}\n`;
      output += `Description: ${tool.description}\n`;
      output += `Parameters:\n`;

      for (const [paramName, paramDef] of Object.entries(tool.parameters.properties)) {
        const required = tool.parameters.required.includes(paramName) ? ' (required)' : ' (optional)';
        output += `  <${paramName}>${required}: ${paramDef.description}\n`;
      }

      output += `\nExample:\n<tool name="${tool.name}">\n`;
      for (const paramName of tool.parameters.required) {
        output += `  <${paramName}>value</${paramName}>\n`;
      }
      output += `</tool>\n\n---\n\n`;
    }

    output += '\nWhen you need to use tools, format them as shown above.\n';
    return output;
  }

  formatResultsForContext(results: Array<{ toolName: string; success: boolean; content?: string; error?: string }>): string {
    return results.map(r => {
      if (r.success && r.content) {
        return `<tool_result>\n<tool_name>${r.toolName}</tool_name>\n<status>success</status>\n<content>\n${r.content}\n</content>\n</tool_result>`;
      } else {
        return `<tool_result>\n<tool_name>${r.toolName}</tool_name>\n<status>error</status>\n<error>${r.error || 'Unknown error'}</error>\n</tool_result>`;
      }
    }).join('\n\n');
  }

  matches(text: string): ToolCallMatch[] {
    const matches: ToolCallMatch[] = [];
    const regex = /<tool\s+name="[^"]+">.*?<\/tool>/gs;

    let match: RegExpExecArray | null;
    while ((match = regex.exec(text)) !== null) {
      matches.push({
        fullMatch: match[0],
        startIndex: match.index,
        endIndex: regex.lastIndex
      });
    }

    return matches;
  }

  parse(match: ToolCallMatch): ToolCall | null {
    const toolName = this.extractAttribute(match.fullMatch, 'name');
    if (!toolName) {
      const preview = match.fullMatch ? match.fullMatch.slice(0, 50) : '[empty match]';
      console.warn(`[OldStyleToolAdapter] Missing name attribute in: ${preview}...`);
      return null;
    }

    // Extract parameters - direct children tags
    const parameters = this.extractParameters(match.fullMatch);

    return { toolName, parameters };
  }
}

/**
 * Adapter for Anthropic-style format: <tool_use><tool_name>...</tool_name><parameters>...</parameters></tool_use>
 */
export class AnthropicStyleToolAdapter extends ToolFormatAdapter {
  readonly formatName = 'anthropic-style';

  formatToolsForPrompt(tools: ToolDefinition[]): string {
    let output = 'Available Tools:\n\n';

    for (const tool of tools) {
      output += `Tool: ${tool.name}\n`;
      output += `Description: ${tool.description}\n`;
      if (tool.category) {
        output += `Category: ${tool.category}\n`;
      }
      output += `\nParameters:\n`;

      for (const [paramName, paramDef] of Object.entries(tool.parameters.properties)) {
        const required = tool.parameters.required.includes(paramName) ? ' (required)' : ' (optional)';
        output += `  - ${paramName}${required}: ${paramDef.description}\n`;
      }

      output += `\n---\n\n`;
    }

    output += `
=== HOW TO CALL TOOLS ===
Use this EXACT XML format to call tools:

<tool_use>
  <tool_name>TOOL_NAME_HERE</tool_name>
  <parameters>
    <param1>value1</param1>
    <param2>value2</param2>
  </parameters>
</tool_use>

Example - voting on a proposal:
<tool_use>
  <tool_name>collaboration/decision/vote</tool_name>
  <parameters>
    <proposalId>uuid-here</proposalId>
    <rankedChoices>["option1-id", "option2-id"]</rankedChoices>
  </parameters>
</tool_use>

CRITICAL RULES:
1. You MUST use the <tool_use> XML format above to call tools
2. NEVER write "Tool 'x' completed" - that is the RESULT format, not how you CALL tools
3. NEVER fabricate or make up tool results - the system will execute your tool call and return actual results
4. If you want to use a tool, output the <tool_use> XML block and WAIT for the result

When you need information, use tools instead of making assumptions.
`;

    return output;
  }

  formatResultsForContext(results: Array<{ toolName: string; success: boolean; content?: string; error?: string }>): string {
    return results.map(r => {
      if (r.success && r.content) {
        return `<tool_result>\n<tool_name>${r.toolName}</tool_name>\n<status>success</status>\n<content>\n${r.content}\n</content>\n</tool_result>`;
      } else {
        return `<tool_result>\n<tool_name>${r.toolName}</tool_name>\n<status>error</status>\n<error>\n\`\`\`\n${r.error || 'Unknown error'}\n\`\`\`\n</error>\n</tool_result>`;
      }
    }).join('\n\n');
  }

  matches(text: string): ToolCallMatch[] {
    const matches: ToolCallMatch[] = [];
    const regex = /<tool_use>.*?<\/tool_use>/gs;

    let match: RegExpExecArray | null;
    while ((match = regex.exec(text)) !== null) {
      matches.push({
        fullMatch: match[0],
        startIndex: match.index,
        endIndex: regex.lastIndex
      });
    }

    return matches;
  }

  parse(match: ToolCallMatch): ToolCall | null {
    const toolName = this.extractTag(match.fullMatch, 'tool_name');
    if (!toolName) {
      const preview = match.fullMatch ? match.fullMatch.slice(0, 50) : '[empty match]';
      console.warn(`[AnthropicStyleToolAdapter] Missing <tool_name> in: ${preview}...`);
      return null;
    }

    // Extract parameters block, then parse parameters from it
    const parametersBlock = this.extractTag(match.fullMatch, 'parameters');
    const parameters = parametersBlock ? this.extractParameters(parametersBlock) : {};

    return { toolName, parameters };
  }
}

/**
 * Adapter for Markdown/Backtick format that local models (llama, etc.) often produce
 * Format: `tool: name` `param=value` `param2=value2`
 *
 * Examples:
 * - `tool: collaboration/dm` `participants=helper`
 * - `tool: read` `filepath=/path/to/file`
 */
export class MarkdownToolAdapter extends ToolFormatAdapter {
  readonly formatName = 'markdown-backtick';

  formatToolsForPrompt(tools: ToolDefinition[]): string {
    // This adapter is for parsing, not prompting - use Anthropic format for prompts
    return '';
  }

  formatResultsForContext(results: Array<{ toolName: string; success: boolean; content?: string; error?: string }>): string {
    // Use XML format for results to clearly distinguish from tool calls
    // This prevents AIs from mimicking the result format when they should be calling tools
    return results.map(r => {
      if (r.success && r.content) {
        return `<tool_result>\n<tool_name>${r.toolName}</tool_name>\n<status>success</status>\n<content>\n${r.content}\n</content>\n</tool_result>`;
      } else {
        return `<tool_result>\n<tool_name>${r.toolName}</tool_name>\n<status>error</status>\n<error>\n${r.error || 'Unknown error'}\n</error>\n</tool_result>`;
      }
    }).join('\n\n');
  }

  matches(text: string): ToolCallMatch[] {
    const matches: ToolCallMatch[] = [];

    // Match patterns like: `tool: name` followed by optional params `key=value`
    // Strategy: Find all `tool: X` occurrences and capture until end of line or next tool
    const lines = text.split('\n');
    let currentMatch = '';
    let startIndex = 0;
    let charOffset = 0;

    for (const line of lines) {
      const toolMatch = line.match(/`tool:\s*[^`]+`/i);

      if (toolMatch) {
        // Save previous match if exists
        if (currentMatch) {
          matches.push({
            fullMatch: currentMatch.trim(),
            startIndex,
            endIndex: charOffset
          });
        }
        // Start new match
        currentMatch = line;
        startIndex = charOffset + (toolMatch.index || 0);
      } else if (currentMatch && line.includes('`') && line.includes('=')) {
        // Continue current match with param line
        currentMatch += ' ' + line;
      }

      charOffset += line.length + 1; // +1 for newline
    }

    // Don't forget last match
    if (currentMatch) {
      matches.push({
        fullMatch: currentMatch.trim(),
        startIndex,
        endIndex: charOffset
      });
    }

    return matches;
  }

  parse(match: ToolCallMatch): ToolCall | null {
    // Extract tool name from first backtick section
    const toolNameMatch = match.fullMatch.match(/`tool:\s*([^`]+)`/i);
    if (!toolNameMatch) {
      return null;
    }

    const toolName = toolNameMatch[1].trim();
    const parameters: Record<string, string> = {};

    // Extract all param=value pairs from subsequent backtick sections
    const paramRegex = /`([^`=]+)=([^`]*)`/g;
    let paramMatch: RegExpExecArray | null;

    while ((paramMatch = paramRegex.exec(match.fullMatch)) !== null) {
      const paramName = paramMatch[1].trim();
      const paramValue = paramMatch[2].trim();
      if (paramName && paramName !== 'tool') {
        parameters[paramName] = paramValue;
      }
    }

    return { toolName, parameters };
  }
}

/**
 * Registry of all supported tool format adapters
 * Add new adapters here to support additional formats
 *
 * Order matters for auto-detection - primary adapter is tried first
 */
export function getToolFormatAdapters(): ToolFormatAdapter[] {
  return [
    new AnthropicStyleToolAdapter(),  // Primary/default format
    new MarkdownToolAdapter(),         // Local model backtick format
    new OldStyleToolAdapter()          // Legacy XML support
  ];
}

/**
 * Get primary adapter for formatting (Anthropic style is our default)
 * Unless Anthropic's format is insufficient, we prefer using their standard
 * rather than inventing our own. All formats are supported via regex adapters.
 *
 * In future, this can be smart selection based on model config/recipe
 */
export function getPrimaryAdapter(): ToolFormatAdapter {
  return getToolFormatAdapters()[0];  // AnthropicStyleToolAdapter
}

// ========================
// Native Tool Support (JSON format for Anthropic API)
// ========================

import type { NativeToolSpec } from '../../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';

/**
 * Sanitize tool name for Anthropic API
 * API requires: ^[a-zA-Z0-9_-]{1,128}$
 * Our tools have slashes like 'data/list', 'collaboration/chat/send'
 */
export function sanitizeToolName(name: string): string {
  // Replace slashes with double underscores (reversible)
  return name.replace(/\//g, '__');
}

/**
 * Restore original tool name from sanitized version
 */
export function unsanitizeToolName(sanitizedName: string): string {
  // Restore slashes from double underscores
  return sanitizedName.replace(/__/g, '/');
}

/**
 * Convert ToolDefinition[] to NativeToolSpec[] for providers that support native JSON tools
 * (Anthropic, OpenAI, etc.)
 *
 * This enables native tool_use instead of XML parsing, which is more reliable.
 */
export function convertToNativeToolSpecs(tools: ToolDefinition[]): NativeToolSpec[] {
  return tools.map(tool => {
    // Convert our ToolDefinition to Anthropic's input_schema format
    const properties: Record<string, { type: string; description?: string; enum?: string[] }> = {};
    const required: string[] = [];

    for (const [paramName, paramDef] of Object.entries(tool.parameters.properties)) {
      properties[paramName] = {
        type: paramDef.type || 'string',
        description: paramDef.description,
      };

      // Check if required
      if (tool.parameters.required.includes(paramName) || paramDef.required) {
        required.push(paramName);
      }
    }

    return {
      // Sanitize name for API (data/list -> data__list)
      name: sanitizeToolName(tool.name),
      description: tool.description,
      input_schema: {
        type: 'object',
        properties,
        required: required.length > 0 ? required : undefined,
      },
    };
  });
}

/**
 * Check if a provider supports native JSON tool calling
 */
export function supportsNativeTools(provider: string): boolean {
  // Providers that support native tool_use JSON format
  const nativeToolProviders = ['anthropic', 'openai', 'azure'];
  return nativeToolProviders.includes(provider.toLowerCase());
}
