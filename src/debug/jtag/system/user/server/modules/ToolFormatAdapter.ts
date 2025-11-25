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
      console.warn(`[OldStyleToolAdapter] Missing name attribute in: ${match.fullMatch.slice(0, 50)}...`);
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
Usage Format (Anthropic Claude XML style):
<tool_use>
  <tool_name>read</tool_name>
  <parameters>
    <filepath>/path/to/file.ts</filepath>
  </parameters>
</tool_use>

When you need information, use tools instead of making assumptions.
Examples:
- Unknown file content? Use tools to read files
- Need to find code? Use tools to search
- Want to verify system state? Use tools to check
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
      console.warn(`[AnthropicStyleToolAdapter] Missing <tool_name> in: ${match.fullMatch.slice(0, 50)}...`);
      return null;
    }

    // Extract parameters block, then parse parameters from it
    const parametersBlock = this.extractTag(match.fullMatch, 'parameters');
    const parameters = parametersBlock ? this.extractParameters(parametersBlock) : {};

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
    new OldStyleToolAdapter()          // Legacy support
    // Add new adapters here for future formats
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
